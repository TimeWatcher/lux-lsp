use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gmod_api_db::{
    ApiDatabase, ApiDocumentPage, ApiDocumentStatus, ApiEntry, ApiExample, ApiKind,
    ApiOverrideSource, ApiParameter, ApiRealm, ApiReturn, ApiSignature, ApiSource, ClassEntry,
    CoverageIssue, CoverageManifest, HookEntry, OFFICIAL_PAGELIST_URL, OfficialPageRef,
};
use regex::Regex;
use serde::Deserialize;

const PARSER_VERSION: &str = "facepunch-json-markup-v3";
const DEFAULT_BASE_URL: &str = "https://wiki.facepunch.com/gmod";

pub fn run_from_env() -> Result<UpdateSummary, String> {
    run_with_args(std::env::args().skip(1))
}

pub fn run_with_args(args: impl IntoIterator<Item = String>) -> Result<UpdateSummary, String> {
    let args = Args::parse(args)?;
    run(args)
}

pub fn update_database(options: UpdateOptions) -> Result<UpdateSummary, String> {
    let args = Args::from_options(options);
    run(args)
}

fn run(args: Args) -> Result<UpdateSummary, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("lux-gmod-api-update/0.1")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| format!("failed to create HTTP client: {err}"))?;

    let page_list_text = fetch_text(&client, &args.source_url)?;
    let mut page_list: Vec<PageListItem> = serde_json::from_str(&page_list_text)
        .map_err(|err| format!("failed to parse official page list JSON: {err}"))?;
    page_list.retain(|page| !page.address.is_empty());
    if let Some(limit) = args.limit {
        page_list.truncate(limit);
    }

    let scraped_at = unix_timestamp();
    let converted = fetch_and_convert_pages(&args, &client, page_list.clone())?;
    let mut documents = Vec::new();
    let mut entries = Vec::new();
    let mut hooks = Vec::new();
    let mut classes = BTreeMap::<String, ClassEntry>::new();
    let mut pages = Vec::new();
    let mut skipped_pages = Vec::new();
    let mut fallback_pages = Vec::new();
    let mut failed_pages = Vec::new();
    let mut api_candidate_count = 0usize;
    let mut structured_page_count = 0usize;
    let mut fallback_page_count = 0usize;
    let mut parsed_page_count = 0usize;

    for item in converted {
        match item {
            PageResult::Parsed(page) => {
                pages.push(page.page_ref.clone());
                if page.api_candidate {
                    api_candidate_count += 1;
                    parsed_page_count += 1;
                    if page.structured {
                        structured_page_count += 1;
                    } else {
                        fallback_page_count += 1;
                        fallback_pages.push(CoverageIssue {
                            address: page.address.clone(),
                            reason:
                                "API candidate kept as fallback documentation page; no structured API markup parsed"
                                    .into(),
                        });
                    }
                } else {
                    skipped_pages.push(CoverageIssue {
                        address: page.address,
                        reason: "official non-API documentation page".into(),
                    });
                }
                documents.push(page.document);
                entries.extend(page.entries);
                hooks.extend(page.hooks);
                for class in page.classes {
                    classes
                        .entry(class.name.clone())
                        .and_modify(|existing| {
                            merge_class_metadata(existing, &class);
                            existing.methods.extend(class.methods.clone());
                        })
                        .or_insert(class);
                }
            }
            PageResult::Failed { issue, page_ref } => {
                pages.push(page_ref);
                failed_pages.push(issue);
            }
        }
    }

    synthesize_parent_entries(&mut entries);
    attach_methods_to_classes(&mut entries, &mut classes);
    dedupe_and_sort(&mut entries, &mut hooks, &mut classes);

    let coverage = CoverageManifest {
        source_url: args.source_url.clone(),
        scraped_at: scraped_at.clone(),
        parser_version: PARSER_VERSION.into(),
        official_page_count: page_list.len(),
        document_page_count: documents.len(),
        api_candidate_count,
        structured_page_count,
        fallback_page_count,
        parsed_page_count,
        skipped_page_count: skipped_pages.len(),
        failed_page_count: failed_pages.len(),
        pages,
        skipped_pages,
        fallback_pages,
        failed_pages,
    };

    if !args.allow_failures && coverage.failed_page_count > 0 {
        return Err(format!(
            "official API update left {} failed page(s); re-run with --allow-failures only for parser development",
            coverage.failed_page_count
        ));
    }
    if !args.allow_failures && coverage.document_page_count != coverage.official_page_count {
        return Err(format!(
            "official API update converted {} document page(s), but the official pagelist contains {}; re-run with --allow-failures only for parser development",
            coverage.document_page_count, coverage.official_page_count
        ));
    }
    if !args.allow_failures && coverage.fallback_page_count > 0 {
        return Err(format!(
            "official API update left {} API candidate page(s) as fallback documentation; re-run with --allow-failures only for parser development",
            coverage.fallback_page_count
        ));
    }

    let mut database = ApiDatabase {
        version: format!("official-{scraped_at}"),
        generated_from: "Facepunch Garry's Mod Wiki JSON".into(),
        generated_at: scraped_at,
        source_url: args.source_url.clone(),
        parser_version: PARSER_VERSION.into(),
        coverage: Some(coverage.clone()),
        overrides: Vec::new(),
        documents,
        entries,
        hooks,
        classes: classes.into_values().collect(),
    };
    apply_override_files(&mut database, &args.overrides)?;

    write_json(&args.out, &database)?;
    if let Some(path) = &args.coverage_out {
        write_json(path, &coverage)?;
    }
    Ok(UpdateSummary {
        entries: database.entries.len(),
        hooks: database.hooks.len(),
        classes: database.classes.len(),
        official_pages: database
            .coverage
            .as_ref()
            .map(|coverage| coverage.official_page_count)
            .unwrap_or_default(),
        document_pages: coverage.document_page_count,
        api_candidate_pages: coverage.api_candidate_count,
        structured_pages: coverage.structured_page_count,
        fallback_pages: coverage.fallback_page_count,
        failed_pages: coverage.failed_page_count,
        database_path: args.out,
        coverage_path: args.coverage_out,
    })
}

#[derive(Debug, Clone)]
pub struct UpdateOptions {
    pub out: PathBuf,
    pub coverage_out: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
    pub source_url: String,
    pub base_url: String,
    pub limit: Option<usize>,
    pub concurrency: usize,
    pub allow_failures: bool,
    pub offline: bool,
    pub overrides: Vec<PathBuf>,
}

impl Default for UpdateOptions {
    fn default() -> Self {
        Self {
            out: PathBuf::from("crates/gmod-api-db/data/generated/gmod_api.json"),
            coverage_out: Some(PathBuf::from(
                "crates/gmod-api-db/data/generated/coverage_manifest.json",
            )),
            cache_dir: Some(PathBuf::from("target/gmod-api-cache")),
            source_url: OFFICIAL_PAGELIST_URL.into(),
            base_url: DEFAULT_BASE_URL.into(),
            limit: None,
            concurrency: 8,
            allow_failures: false,
            offline: false,
            overrides: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSummary {
    pub entries: usize,
    pub hooks: usize,
    pub classes: usize,
    pub official_pages: usize,
    pub document_pages: usize,
    pub api_candidate_pages: usize,
    pub structured_pages: usize,
    pub fallback_pages: usize,
    pub failed_pages: usize,
    pub database_path: PathBuf,
    pub coverage_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct Args {
    out: PathBuf,
    coverage_out: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    source_url: String,
    base_url: String,
    limit: Option<usize>,
    concurrency: usize,
    allow_failures: bool,
    offline: bool,
    overrides: Vec<PathBuf>,
}

impl Args {
    fn from_options(options: UpdateOptions) -> Self {
        Self {
            out: options.out,
            coverage_out: options.coverage_out,
            cache_dir: options.cache_dir,
            source_url: options.source_url,
            base_url: options.base_url,
            limit: options.limit,
            concurrency: options.concurrency.max(1),
            allow_failures: options.allow_failures,
            offline: options.offline,
            overrides: options.overrides,
        }
    }

    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let mut parsed = Self::from_options(UpdateOptions::default());
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--out" => parsed.out = required_path(&mut args, "--out")?,
                "--coverage-out" => {
                    parsed.coverage_out = Some(required_path(&mut args, "--coverage-out")?)
                }
                "--no-coverage-out" => parsed.coverage_out = None,
                "--cache-dir" => parsed.cache_dir = Some(required_path(&mut args, "--cache-dir")?),
                "--no-cache" => parsed.cache_dir = None,
                "--source-url" => parsed.source_url = required_value(&mut args, "--source-url")?,
                "--base-url" => parsed.base_url = required_value(&mut args, "--base-url")?,
                "--override" => parsed
                    .overrides
                    .push(required_path(&mut args, "--override")?),
                "--limit" => {
                    parsed.limit = Some(
                        required_value(&mut args, "--limit")?
                            .parse()
                            .map_err(|err| format!("invalid --limit: {err}"))?,
                    )
                }
                "--concurrency" => {
                    parsed.concurrency = required_value(&mut args, "--concurrency")?
                        .parse::<usize>()
                        .map_err(|err| format!("invalid --concurrency: {err}"))?
                        .max(1);
                }
                "--allow-failures" => parsed.allow_failures = true,
                "--offline" => parsed.offline = true,
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument `{other}`")),
            }
        }
        Ok(parsed)
    }
}

fn required_path(args: &mut impl Iterator<Item = String>, name: &str) -> Result<PathBuf, String> {
    required_value(args, name).map(PathBuf::from)
}

fn required_value(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{name} requires a value"))
}

fn print_help() {
    println!(
        "Usage: gmod-api-update [--out PATH] [--coverage-out PATH] [--cache-dir PATH]\n\
         \n\
         Fetches the official Facepunch Garry's Mod Wiki page list, downloads every\n\
         official page JSON payload, converts every page into a document record,\n\
         extracts structured API data, and writes a coverage manifest. Defaults are\n\
         relative to the lux-lsp workspace.\n\
         \n\
         Options:\n\
           --out PATH             Database output path\n\
           --coverage-out PATH    Coverage manifest output path\n\
           --cache-dir PATH       Raw page JSON cache directory\n\
           --source-url URL       Official page list JSON URL\n\
           --base-url URL         Official wiki base URL\n\
           --override PATH        JSON override database to merge after official data\n\
           --limit N              Fetch only N pages for parser development\n\
           --concurrency N        Number of parallel page fetch workers, default 8\n\
           --allow-failures       Write output even if official API pages fail\n\
           --offline              Read page JSON only from cache\n"
    );
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageListItem {
    address: String,
    update_count: Option<u64>,
    view_count: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WikiPage {
    title: String,
    tags: String,
    address: String,
    markup: String,
    update_count: Option<u64>,
}

#[derive(Debug)]
enum PageResult {
    Parsed(ConvertedPage),
    Failed {
        issue: CoverageIssue,
        page_ref: OfficialPageRef,
    },
}

#[derive(Debug)]
struct ConvertedPage {
    address: String,
    page_ref: OfficialPageRef,
    document: ApiDocumentPage,
    api_candidate: bool,
    structured: bool,
    entries: Vec<ApiEntry>,
    hooks: Vec<HookEntry>,
    classes: Vec<ClassEntry>,
}

fn fetch_and_convert_pages(
    args: &Args,
    client: &reqwest::blocking::Client,
    page_list: Vec<PageListItem>,
) -> Result<Vec<PageResult>, String> {
    let total = page_list.len();
    let queue = Arc::new(Mutex::new(
        page_list
            .into_iter()
            .enumerate()
            .collect::<VecDeque<(usize, PageListItem)>>(),
    ));
    let results = Arc::new(Mutex::new(
        (0..total)
            .map(|_| None)
            .collect::<Vec<Option<PageResult>>>(),
    ));

    std::thread::scope(|scope| {
        for _ in 0..args.concurrency {
            let queue = Arc::clone(&queue);
            let results = Arc::clone(&results);
            let args = args.clone();
            let client = client.clone();
            scope.spawn(move || {
                loop {
                    let Some((index, item)) = queue.lock().expect("queue lock").pop_front() else {
                        break;
                    };
                    let result = match read_or_fetch_page(&args, &client, &item) {
                        Ok(text) => match serde_json::from_str::<WikiPage>(&text) {
                            Ok(page) => PageResult::Parsed(convert_page(&args, &item, page)),
                            Err(err) => PageResult::Failed {
                                issue: CoverageIssue {
                                    address: item.address.clone(),
                                    reason: format!("invalid page JSON: {err}"),
                                },
                                page_ref: page_ref_from_list_item(&item),
                            },
                        },
                        Err(reason) => PageResult::Failed {
                            issue: CoverageIssue {
                                address: item.address.clone(),
                                reason,
                            },
                            page_ref: page_ref_from_list_item(&item),
                        },
                    };
                    results.lock().expect("results lock")[index] = Some(result);
                }
            });
        }
    });

    Ok(results
        .lock()
        .expect("results lock")
        .iter_mut()
        .map(|item| item.take().expect("worker filled every result"))
        .collect())
}

fn read_or_fetch_page(
    args: &Args,
    client: &reqwest::blocking::Client,
    item: &PageListItem,
) -> Result<String, String> {
    if let Some(cache_dir) = &args.cache_dir {
        fs::create_dir_all(cache_dir).map_err(|err| {
            format!(
                "failed to create cache dir `{}`: {err}",
                cache_dir.display()
            )
        })?;
        let cache_path = cache_dir.join(cache_file_name(&item.address));
        if cache_path.exists() {
            return fs::read_to_string(&cache_path)
                .map_err(|err| format!("failed to read cache `{}`: {err}", cache_path.display()));
        }
        if args.offline {
            return Err(format!("missing cache entry `{}`", cache_path.display()));
        }
        let url = format!(
            "{}/{}?format=json",
            args.base_url.trim_end_matches('/'),
            encode_path(&item.address)
        );
        let text = fetch_text(client, &url)?;
        fs::write(&cache_path, text.as_bytes())
            .map_err(|err| format!("failed to write cache `{}`: {err}", cache_path.display()))?;
        Ok(text)
    } else {
        if args.offline {
            return Err("--offline requires --cache-dir".into());
        }
        let url = format!(
            "{}/{}?format=json",
            args.base_url.trim_end_matches('/'),
            encode_path(&item.address)
        );
        fetch_text(client, &url)
    }
}

fn fetch_text(client: &reqwest::blocking::Client, url: &str) -> Result<String, String> {
    client
        .get(url)
        .send()
        .map_err(|err| format!("failed to fetch `{url}`: {err}"))?
        .error_for_status()
        .map_err(|err| format!("HTTP error for `{url}`: {err}"))?
        .text()
        .map_err(|err| format!("failed to read response body from `{url}`: {err}"))
}

fn convert_page(args: &Args, list_item: &PageListItem, page: WikiPage) -> ConvertedPage {
    let source = source_for(args, list_item, &page);
    let api_candidate = is_api_candidate(&page);
    let mut converted = ConvertedPage {
        address: page.address.clone(),
        page_ref: OfficialPageRef {
            address: page.address.clone(),
            tags: page.tags.clone(),
            update_count: page.update_count.or(list_item.update_count),
            view_count: list_item.view_count,
        },
        document: ApiDocumentPage {
            address: page.address.clone(),
            title: page.title.clone(),
            tags: page.tags.clone(),
            status: ApiDocumentStatus::Documentation,
            api_candidate,
            structured: false,
            summary: String::new(),
            description: Vec::new(),
            warnings: Vec::new(),
            notes: Vec::new(),
            examples: Vec::new(),
            related: Vec::new(),
            entry_paths: Vec::new(),
            hook_names: Vec::new(),
            class_names: Vec::new(),
            official_url: Some(source.url.clone()),
            source: Some(source.clone()),
        },
        api_candidate,
        structured: false,
        entries: Vec::new(),
        hooks: Vec::new(),
        classes: Vec::new(),
    };
    let type_entries = parse_type_blocks(&page, &source);
    let type_structured = !type_entries.is_empty();
    converted.entries.extend(type_entries);
    let type_classes = parse_type_class_blocks(&page, &source);
    let type_class_structured = !type_classes.is_empty();
    converted.classes.extend(type_classes);
    let (panel_entries, panel_classes) = parse_panel_blocks(&page, &source);
    let panel_structured = !panel_entries.is_empty() || !panel_classes.is_empty();
    converted.entries.extend(panel_entries);
    converted.classes.extend(panel_classes);
    let (function_entries, hooks) = parse_function_blocks(&page, &source);
    let function_structured = !function_entries.is_empty() || !hooks.is_empty();
    converted.entries.extend(function_entries);
    converted.hooks.extend(hooks);
    let enum_entries = parse_enum_blocks(&page, &source);
    let enum_structured = !enum_entries.is_empty();
    converted.entries.extend(enum_entries);
    converted
        .entries
        .extend(parse_structure_blocks(&page, &source));
    let structure_structured = converted
        .entries
        .iter()
        .any(|entry| matches!(entry.kind, ApiKind::Struct | ApiKind::Field));
    converted.structured = type_structured
        || type_class_structured
        || panel_structured
        || function_structured
        || enum_structured
        || structure_structured;

    if converted.api_candidate
        && converted.entries.is_empty()
        && converted.hooks.is_empty()
        && converted.classes.is_empty()
    {
        converted.entries.push(fallback_page_entry(&page, &source));
    }
    converted.document = document_page_for(&page, &source, &converted);
    converted
}

fn page_ref_from_list_item(item: &PageListItem) -> OfficialPageRef {
    OfficialPageRef {
        address: item.address.clone(),
        tags: String::new(),
        update_count: item.update_count,
        view_count: item.view_count,
    }
}

fn source_for(args: &Args, list_item: &PageListItem, page: &WikiPage) -> ApiSource {
    ApiSource {
        address: page.address.clone(),
        url: format!(
            "{}/{}",
            args.base_url.trim_end_matches('/'),
            encode_path(&page.address)
        ),
        tags: page.tags.clone(),
        update_count: page.update_count.or(list_item.update_count),
        view_count: list_item.view_count,
    }
}

fn is_api_candidate(page: &WikiPage) -> bool {
    let tags = page.tags.to_ascii_lowercase();
    tags.split_whitespace().any(|tag| {
        matches!(
            tag,
            "function"
                | "method"
                | "member"
                | "event"
                | "enum"
                | "struct"
                | "type"
                | "panel"
                | "realm-client"
                | "realm-server"
                | "realm-menu"
        )
    }) || has_api_markup_block(&page.markup)
}

fn has_api_markup_block(markup: &str) -> bool {
    static API_MARKUP_RE: OnceLock<Regex> = OnceLock::new();
    API_MARKUP_RE
        .get_or_init(|| Regex::new(r#"(?is)<(function|enum|structure|type|panel)\b"#).unwrap())
        .is_match(markup)
}

fn parse_type_blocks(page: &WikiPage, source: &ApiSource) -> Vec<ApiEntry> {
    tag_blocks(&page.markup, "type")
        .into_iter()
        .filter_map(|block| {
            let attrs = attributes(&block.attrs);
            let name = attrs.get("name")?.trim().to_string();
            let is = attrs.get("is").map(String::as_str).unwrap_or_default();
            let category = attrs
                .get("category")
                .map(String::as_str)
                .unwrap_or_default();
            let kind = match is {
                "library" => ApiKind::Library,
                "class" if name.starts_with('D') && category == "panelfunc" => ApiKind::Panel,
                "class" => ApiKind::Class,
                _ => ApiKind::Page,
            };
            let summary = first_tag_raw(&block.body, "summary")
                .map(|text| clean_markup(&text))
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| clean_markup(&page.markup));
            Some(ApiEntry {
                path: name,
                kind,
                realm: parse_realm(None, &page.tags),
                summary: summary_from(&summary),
                description: paragraphs(&summary),
                signatures: Vec::new(),
                warnings: extract_named_sections(&summary, "warning"),
                notes: extract_named_sections(&summary, "note"),
                examples: parse_examples(&page.markup),
                related: related_pages(&page.markup),
                official_url: Some(source.url.clone()),
                source: Some(source.clone()),
            })
        })
        .collect()
}

fn parse_type_class_blocks(page: &WikiPage, source: &ApiSource) -> Vec<ClassEntry> {
    tag_blocks(&page.markup, "type")
        .into_iter()
        .filter_map(|block| {
            let attrs = attributes(&block.attrs);
            let name = attrs.get("name")?.trim().to_string();
            let is = attrs.get("is").map(String::as_str).unwrap_or_default();
            if is != "class" {
                return None;
            }
            let category = attrs
                .get("category")
                .map(String::as_str)
                .unwrap_or_default();
            let kind = if name.starts_with('D') && category == "panelfunc" {
                ApiKind::Panel
            } else {
                ApiKind::Class
            };
            let summary_raw = first_tag_raw(&block.body, "summary").unwrap_or_default();
            let summary = clean_markup(&summary_raw);
            let parent = attrs
                .get("parent")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            Some(class_entry(
                name,
                kind,
                parse_realm(None, &page.tags),
                parent,
                summary,
                extract_named_sections(&summary_raw, "warning"),
                extract_named_sections(&summary_raw, "note"),
                parse_examples(&page.markup),
                related_pages(&page.markup),
                source,
            ))
        })
        .collect()
}

fn parse_panel_blocks(page: &WikiPage, source: &ApiSource) -> (Vec<ApiEntry>, Vec<ClassEntry>) {
    let mut entries = Vec::new();
    let mut classes = Vec::new();
    for block in tag_blocks(&page.markup, "panel") {
        let name = page.address.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let realm_raw = first_tag_raw(&block.body, "realm");
        let realm = parse_realm(realm_raw.as_deref(), &page.tags);
        let description_raw = first_tag_raw(&block.body, "description").unwrap_or_default();
        let description_without_admonitions = remove_admonitions(&description_raw);
        let description = clean_markup(&description_without_admonitions);
        let warnings = extract_named_sections(&description_raw, "warning");
        let notes = extract_named_sections(&description_raw, "note");
        let parent = first_tag_raw(&block.body, "parent")
            .map(|text| clean_markup(&text))
            .filter(|text| !text.is_empty());
        let examples = parse_examples(&page.markup);
        let mut related = related_pages(&page.markup);
        if let Some(parent) = &parent
            && !related.iter().any(|item| item == parent)
        {
            related.push(parent.clone());
        }
        entries.push(ApiEntry {
            path: name.clone(),
            kind: ApiKind::Panel,
            realm,
            summary: summary_from(&description).if_empty(|| format!("Garry's Mod panel `{name}`.")),
            description: paragraphs(&description),
            signatures: Vec::new(),
            warnings: warnings.clone(),
            notes: notes.clone(),
            examples: examples.clone(),
            related: related.clone(),
            official_url: Some(source.url.clone()),
            source: Some(source.clone()),
        });
        classes.push(class_entry(
            name,
            ApiKind::Panel,
            realm,
            parent,
            description,
            warnings,
            notes,
            examples,
            related,
            source,
        ));
    }
    (entries, classes)
}

fn class_entry(
    name: String,
    kind: ApiKind,
    realm: ApiRealm,
    parent: Option<String>,
    description: String,
    warnings: Vec<String>,
    notes: Vec<String>,
    examples: Vec<ApiExample>,
    related: Vec<String>,
    source: &ApiSource,
) -> ClassEntry {
    ClassEntry {
        name: name.clone(),
        summary: summary_from(&description).if_empty(|| format!("Garry's Mod class `{name}`.")),
        realm: Some(realm),
        parent,
        kind: Some(kind),
        description: paragraphs(&description),
        methods: Vec::new(),
        warnings,
        notes,
        examples,
        related,
        official_url: Some(source.url.clone()),
        source: Some(source.clone()),
    }
}

fn parse_function_blocks(page: &WikiPage, source: &ApiSource) -> (Vec<ApiEntry>, Vec<HookEntry>) {
    let mut entries = Vec::new();
    let mut hooks = Vec::new();
    for block in tag_blocks(&page.markup, "function") {
        let attrs = attributes(&block.attrs);
        let Some(name) = attrs.get("name").cloned() else {
            continue;
        };
        let parent = attrs.get("parent").map(String::as_str).unwrap_or("Global");
        let function_type = attrs.get("type").map(String::as_str).unwrap_or_default();
        let realm_raw = first_tag_raw(&block.body, "realm");
        let realm = parse_realm(realm_raw.as_deref(), &page.tags);
        let description_raw = first_tag_raw(&block.body, "description").unwrap_or_default();
        let description_without_admonitions = remove_admonitions(&description_raw);
        let description = clean_markup(&description_without_admonitions);
        let warnings = extract_named_sections(&description_raw, "warning");
        let notes = extract_named_sections(&description_raw, "note");
        let parameters = parse_parameters(&block.body);
        let returns = parse_returns(&block.body);
        let path = function_path(parent, &name, function_type, &page.address);
        let signature = ApiSignature {
            label: signature_label(&path, &parameters),
            parameters,
            returns,
        };
        let kind = match function_type {
            "hook" => ApiKind::Hook,
            "classfunc" | "panelfunc" => ApiKind::Method,
            _ if parent == "Global" => ApiKind::Function,
            _ => ApiKind::Function,
        };
        let entry = ApiEntry {
            path: path.clone(),
            kind,
            realm,
            summary: summary_from(&description),
            description: paragraphs(&description),
            signatures: vec![signature.clone()],
            warnings: warnings.clone(),
            notes: notes.clone(),
            examples: parse_examples(&page.markup),
            related: related_pages(&page.markup),
            official_url: Some(source.url.clone()),
            source: Some(source.clone()),
        };
        if function_type == "hook" || parent == "GM" {
            hooks.push(HookEntry {
                name: name.clone(),
                gm_path: path.clone(),
                realm,
                summary: entry.summary.clone(),
                description: entry.description.clone(),
                callback: signature,
                warnings,
                notes,
                examples: entry.examples.clone(),
                official_url: Some(source.url.clone()),
                source: Some(source.clone()),
            });
        }
        entries.push(entry);
    }
    (entries, hooks)
}

fn parse_enum_blocks(page: &WikiPage, source: &ApiSource) -> Vec<ApiEntry> {
    let mut entries = Vec::new();
    for block in tag_blocks(&page.markup, "enum") {
        let realm_raw = first_tag_raw(&block.body, "realm");
        let realm = parse_realm(realm_raw.as_deref(), &page.tags);
        let description_raw = first_tag_raw(&block.body, "description").unwrap_or_default();
        let description = clean_markup(&description_raw);
        let enum_name = page.address.strip_prefix("Enums/").unwrap_or(&page.title);
        entries.push(ApiEntry {
            path: enum_name.to_string(),
            kind: ApiKind::Enum,
            realm,
            summary: summary_from(&description),
            description: paragraphs(&description),
            signatures: Vec::new(),
            warnings: extract_named_sections(&description_raw, "warning"),
            notes: extract_named_sections(&description_raw, "note"),
            examples: parse_examples(&page.markup),
            related: related_pages(&page.markup),
            official_url: Some(source.url.clone()),
            source: Some(source.clone()),
        });
        for item in tag_blocks(&block.body, "item") {
            let attrs = attributes(&item.attrs);
            let Some(key) = attrs.get("key").cloned() else {
                continue;
            };
            let value = attrs.get("value").cloned();
            let item_description = clean_markup(&item.body);
            let mut signature = Vec::new();
            if let Some(value) = value.clone() {
                signature.push(ApiSignature {
                    label: format!("{key} = {value}"),
                    parameters: Vec::new(),
                    returns: Vec::new(),
                });
            }
            entries.push(ApiEntry {
                path: key,
                kind: ApiKind::Constant,
                realm,
                summary: summary_from(&item_description).if_empty(|| {
                    value
                        .as_ref()
                        .map(|value| format!("{enum_name} enum value `{value}`"))
                        .unwrap_or_else(|| format!("{enum_name} enum value"))
                }),
                description: paragraphs(&item_description),
                signatures: signature,
                warnings: Vec::new(),
                notes: Vec::new(),
                examples: Vec::new(),
                related: vec![enum_name.to_string()],
                official_url: Some(source.url.clone()),
                source: Some(source.clone()),
            });
        }
    }
    entries
}

fn parse_structure_blocks(page: &WikiPage, source: &ApiSource) -> Vec<ApiEntry> {
    let mut entries = Vec::new();
    for block in tag_blocks(&page.markup, "structure") {
        let realm_raw = first_tag_raw(&block.body, "realm");
        let realm = parse_realm(realm_raw.as_deref(), &page.tags);
        let description_raw = first_tag_raw(&block.body, "description").unwrap_or_default();
        let description = clean_markup(&description_raw);
        let struct_name = page
            .address
            .strip_prefix("Structures/")
            .unwrap_or(&page.title)
            .to_string();
        entries.push(ApiEntry {
            path: struct_name.clone(),
            kind: ApiKind::Struct,
            realm,
            summary: summary_from(&description),
            description: paragraphs(&description),
            signatures: Vec::new(),
            warnings: extract_named_sections(&description_raw, "warning"),
            notes: extract_named_sections(&description_raw, "note"),
            examples: parse_examples(&page.markup),
            related: related_pages(&page.markup),
            official_url: Some(source.url.clone()),
            source: Some(source.clone()),
        });
        for field in tag_blocks(&block.body, "item") {
            let attrs = attributes(&field.attrs);
            let Some(name) = attrs.get("name").cloned() else {
                continue;
            };
            let ty = attrs.get("type").cloned().unwrap_or_else(|| "any".into());
            let default = attrs.get("default").cloned();
            let field_description = clean_markup(&field.body);
            entries.push(ApiEntry {
                path: format!("{struct_name}.{name}"),
                kind: ApiKind::Field,
                realm,
                summary: summary_from(&field_description),
                description: paragraphs(&field_description),
                signatures: vec![ApiSignature {
                    label: default
                        .as_ref()
                        .map(|value| format!("{name}: {ty} = {value}"))
                        .unwrap_or_else(|| format!("{name}: {ty}")),
                    parameters: Vec::new(),
                    returns: vec![ApiReturn {
                        name: name.clone(),
                        ty,
                        description: field_description.clone(),
                    }],
                }],
                warnings: Vec::new(),
                notes: Vec::new(),
                examples: Vec::new(),
                related: vec![struct_name.clone()],
                official_url: Some(source.url.clone()),
                source: Some(source.clone()),
            });
        }
    }
    entries
}

fn fallback_page_entry(page: &WikiPage, source: &ApiSource) -> ApiEntry {
    let text = clean_markup(&page.markup);
    ApiEntry {
        path: fallback_path(page),
        kind: fallback_kind(page),
        realm: parse_realm(None, &page.tags),
        summary: summary_from(&text),
        description: paragraphs(&text),
        signatures: Vec::new(),
        warnings: extract_named_sections(&page.markup, "warning"),
        notes: extract_named_sections(&page.markup, "note"),
        examples: parse_examples(&page.markup),
        related: related_pages(&page.markup),
        official_url: Some(source.url.clone()),
        source: Some(source.clone()),
    }
}

fn document_page_for(
    page: &WikiPage,
    source: &ApiSource,
    converted: &ConvertedPage,
) -> ApiDocumentPage {
    let text = clean_markup(&page.markup);
    let status = if converted.structured {
        ApiDocumentStatus::StructuredApi
    } else if converted.api_candidate {
        ApiDocumentStatus::ApiFallback
    } else {
        ApiDocumentStatus::Documentation
    };
    let mut entry_paths = converted
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let hook_names = converted
        .hooks
        .iter()
        .map(|hook| hook.name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let class_names = converted
        .classes
        .iter()
        .map(|class| class.name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    entry_paths.sort();
    ApiDocumentPage {
        address: page.address.clone(),
        title: page.title.clone(),
        tags: page.tags.clone(),
        status,
        api_candidate: converted.api_candidate,
        structured: converted.structured,
        summary: summary_from(&text).if_empty(|| page.title.clone()),
        description: paragraphs(&text),
        warnings: extract_named_sections(&page.markup, "warning"),
        notes: extract_named_sections(&page.markup, "note"),
        examples: parse_examples(&page.markup),
        related: related_pages(&page.markup),
        entry_paths,
        hook_names,
        class_names,
        official_url: Some(source.url.clone()),
        source: Some(source.clone()),
    }
}

fn fallback_path(page: &WikiPage) -> String {
    page.address
        .strip_prefix("Global.")
        .or_else(|| page.address.strip_prefix("Enums/"))
        .or_else(|| page.address.strip_prefix("Structures/"))
        .unwrap_or(&page.address)
        .to_string()
}

fn fallback_kind(page: &WikiPage) -> ApiKind {
    let tags = page.tags.to_ascii_lowercase();
    if tags.contains("enum") {
        ApiKind::Enum
    } else if tags.contains("struct") {
        ApiKind::Struct
    } else if tags.contains("type") {
        ApiKind::Page
    } else if tags.contains("event") {
        ApiKind::Hook
    } else if tags.contains("member") || tags.contains("method") {
        ApiKind::Method
    } else {
        ApiKind::Page
    }
}

fn function_path(parent: &str, name: &str, function_type: &str, address: &str) -> String {
    if parent == "Global" || parent.is_empty() {
        return name.to_string();
    }
    if address.contains(':')
        || matches!(function_type, "classfunc" | "panelfunc")
        || function_type == "hook"
    {
        format!("{parent}:{name}")
    } else {
        format!("{parent}.{name}")
    }
}

fn signature_label(path: &str, parameters: &[ApiParameter]) -> String {
    let args = parameters
        .iter()
        .map(|parameter| {
            parameter
                .default
                .as_ref()
                .map(|default| format!("{} = {default}", parameter.name))
                .unwrap_or_else(|| parameter.name.clone())
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{path}({args})")
}

fn parse_parameters(body: &str) -> Vec<ApiParameter> {
    first_tag_raw(body, "args")
        .map(|args| {
            tag_blocks(&args, "arg")
                .into_iter()
                .filter_map(|block| {
                    let attrs = attributes(&block.attrs);
                    let name = attrs.get("name")?.clone();
                    let default = attrs.get("default").cloned();
                    let callback = parse_callback_signature(&name, &block.body);
                    let description_raw = remove_tag_blocks(&block.body, "callback");
                    Some(ApiParameter {
                        name,
                        ty: attrs.get("type").cloned().unwrap_or_else(|| "any".into()),
                        description: clean_markup(&description_raw),
                        optional: default.is_some(),
                        default,
                        callback,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_callback_signature(name: &str, body: &str) -> Option<ApiSignature> {
    let callback = first_tag_raw(body, "callback")?;
    let parameters = tag_blocks(&callback, "arg")
        .into_iter()
        .filter_map(|block| {
            let attrs = attributes(&block.attrs);
            let name = attrs.get("name")?.clone();
            let default = attrs.get("default").cloned();
            Some(ApiParameter {
                name,
                ty: attrs.get("type").cloned().unwrap_or_else(|| "any".into()),
                description: clean_markup(&block.body),
                optional: default.is_some(),
                default,
                callback: None,
            })
        })
        .collect::<Vec<_>>();
    let returns = tag_blocks(&callback, "ret")
        .into_iter()
        .filter_map(|block| {
            let attrs = attributes(&block.attrs);
            Some(ApiReturn {
                name: attrs.get("name").cloned().unwrap_or_default(),
                ty: attrs.get("type").cloned().unwrap_or_else(|| "any".into()),
                description: clean_markup(&block.body),
            })
        })
        .collect::<Vec<_>>();
    if parameters.is_empty() && returns.is_empty() {
        return None;
    }
    Some(ApiSignature {
        label: signature_label(name, &parameters),
        parameters,
        returns,
    })
}

fn parse_returns(body: &str) -> Vec<ApiReturn> {
    first_tag_raw(body, "rets")
        .map(|rets| {
            tag_blocks(&rets, "ret")
                .into_iter()
                .filter_map(|block| {
                    let attrs = attributes(&block.attrs);
                    Some(ApiReturn {
                        name: attrs.get("name").cloned().unwrap_or_default(),
                        ty: attrs.get("type").cloned().unwrap_or_else(|| "any".into()),
                        description: clean_markup(&block.body),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_examples(markup: &str) -> Vec<ApiExample> {
    tag_blocks(markup, "example")
        .into_iter()
        .filter_map(|block| {
            let description = first_tag_raw(&block.body, "description")
                .map(|text| clean_markup(&text))
                .unwrap_or_default();
            let code = first_tag_raw(&block.body, "code")
                .map(|text| trim_code(&text))
                .unwrap_or_default();
            if code.is_empty() {
                return None;
            }
            let output = first_tag_raw(&block.body, "output")
                .map(|text| clean_markup(&text))
                .unwrap_or_default();
            Some(ApiExample {
                title: summary_from(&description),
                language: "lua".into(),
                code,
                description,
                output,
            })
        })
        .collect()
}

fn parse_realm(realm_text: Option<&str>, tags: &str) -> ApiRealm {
    let mut text = realm_text.unwrap_or_default().to_ascii_lowercase();
    text.push(' ');
    text.push_str(&tags.to_ascii_lowercase());
    let client = text.contains("client") || text.contains("realm-client");
    let server = text.contains("server") || text.contains("realm-server");
    let shared = text.contains("shared") || (client && server);
    let menu = text.contains("menu") || text.contains("realm-menu");
    if shared {
        ApiRealm::Shared
    } else if server {
        ApiRealm::Server
    } else if client {
        ApiRealm::Client
    } else if menu {
        ApiRealm::Menu
    } else {
        ApiRealm::Shared
    }
}

fn synthesize_parent_entries(entries: &mut Vec<ApiEntry>) {
    let existing = entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>();
    let mut parents = BTreeMap::<String, Vec<ApiRealm>>::new();
    for entry in entries.iter() {
        if let Some(index) = entry.path.find(['.', ':']) {
            parents
                .entry(entry.path[..index].to_string())
                .or_default()
                .push(entry.realm);
        }
    }
    for (parent, realms) in parents {
        if existing.contains(&parent) {
            continue;
        }
        entries.push(ApiEntry {
            path: parent.clone(),
            kind: ApiKind::Library,
            realm: combined_realm(&realms),
            summary: format!(
                "Garry's Mod API namespace `{parent}` generated from official member pages."
            ),
            description: Vec::new(),
            signatures: Vec::new(),
            warnings: Vec::new(),
            notes: Vec::new(),
            examples: Vec::new(),
            related: Vec::new(),
            official_url: Some(format!("{DEFAULT_BASE_URL}/{parent}")),
            source: None,
        });
    }
}

fn combined_realm(realms: &[ApiRealm]) -> ApiRealm {
    let has_client = realms
        .iter()
        .any(|realm| matches!(realm, ApiRealm::Client | ApiRealm::Shared));
    let has_server = realms
        .iter()
        .any(|realm| matches!(realm, ApiRealm::Server | ApiRealm::Shared));
    match (has_client, has_server) {
        (true, true) => ApiRealm::Shared,
        (true, false) => ApiRealm::Client,
        (false, true) => ApiRealm::Server,
        (false, false) => ApiRealm::Menu,
    }
}

fn attach_methods_to_classes(entries: &mut [ApiEntry], classes: &mut BTreeMap<String, ClassEntry>) {
    for entry in entries.iter() {
        let Some(index) = entry.path.find(':') else {
            continue;
        };
        let class_name = entry.path[..index].to_string();
        classes
            .entry(class_name.clone())
            .or_insert_with(|| ClassEntry {
                name: class_name.clone(),
                summary: format!(
                    "Garry's Mod class `{class_name}` generated from official method pages."
                ),
                realm: None,
                parent: None,
                kind: Some(ApiKind::Class),
                description: Vec::new(),
                methods: Vec::new(),
                warnings: Vec::new(),
                notes: Vec::new(),
                examples: Vec::new(),
                related: Vec::new(),
                official_url: Some(format!("{DEFAULT_BASE_URL}/{class_name}")),
                source: None,
            })
            .methods
            .push(entry.clone());
    }
}

fn merge_class_metadata(existing: &mut ClassEntry, incoming: &ClassEntry) {
    if existing.summary.is_empty() || existing.summary.starts_with("Garry's Mod class `") {
        existing.summary = incoming.summary.clone();
    }
    if existing.realm.is_none() {
        existing.realm = incoming.realm;
    }
    if existing.parent.is_none() {
        existing.parent = incoming.parent.clone();
    }
    if existing.kind.is_none() {
        existing.kind = incoming.kind;
    }
    if existing.description.is_empty() {
        existing.description = incoming.description.clone();
    }
    if existing.warnings.is_empty() {
        existing.warnings = incoming.warnings.clone();
    }
    if existing.notes.is_empty() {
        existing.notes = incoming.notes.clone();
    }
    if existing.examples.is_empty() {
        existing.examples = incoming.examples.clone();
    }
    if existing.related.is_empty() {
        existing.related = incoming.related.clone();
    }
    if existing.official_url.is_none() {
        existing.official_url = incoming.official_url.clone();
    }
    if existing.source.is_none() {
        existing.source = incoming.source.clone();
    }
}

fn dedupe_and_sort(
    entries: &mut Vec<ApiEntry>,
    hooks: &mut Vec<HookEntry>,
    classes: &mut BTreeMap<String, ClassEntry>,
) {
    let mut by_path = BTreeMap::<String, ApiEntry>::new();
    for entry in entries.drain(..) {
        by_path.entry(entry.path.clone()).or_insert(entry);
    }
    entries.extend(by_path.into_values());

    let mut by_hook = BTreeMap::<String, HookEntry>::new();
    for hook in hooks.drain(..) {
        by_hook.entry(hook.name.clone()).or_insert(hook);
    }
    hooks.extend(by_hook.into_values());

    for class in classes.values_mut() {
        let mut methods = BTreeMap::<String, ApiEntry>::new();
        for method in class.methods.drain(..) {
            methods.entry(method.path.clone()).or_insert(method);
        }
        class.methods.extend(methods.into_values());
    }
}

fn apply_override_files(database: &mut ApiDatabase, paths: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        let text = fs::read_to_string(path)
            .map_err(|err| format!("failed to read override `{}`: {err}", path.display()))?;
        let override_db: ApiOverrideFile = serde_json::from_str(&text)
            .map_err(|err| format!("failed to parse override `{}`: {err}", path.display()))?;
        merge_entries_by_path(&mut database.entries, override_db.entries);
        merge_hooks_by_name(&mut database.hooks, override_db.hooks);
        merge_classes_by_name(&mut database.classes, override_db.classes);
        database.overrides.push(ApiOverrideSource {
            path: path.display().to_string(),
            version: override_db.version.unwrap_or_else(|| "unversioned".into()),
        });
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ApiOverrideFile {
    version: Option<String>,
    #[serde(default)]
    entries: Vec<ApiEntry>,
    #[serde(default)]
    hooks: Vec<HookEntry>,
    #[serde(default)]
    classes: Vec<ClassEntry>,
}

fn merge_entries_by_path(target: &mut Vec<ApiEntry>, overrides: Vec<ApiEntry>) {
    let mut by_path = target
        .drain(..)
        .map(|entry| (entry.path.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    for entry in overrides {
        by_path.insert(entry.path.clone(), entry);
    }
    target.extend(by_path.into_values());
}

fn merge_hooks_by_name(target: &mut Vec<HookEntry>, overrides: Vec<HookEntry>) {
    let mut by_name = target
        .drain(..)
        .map(|hook| (hook.name.clone(), hook))
        .collect::<BTreeMap<_, _>>();
    for hook in overrides {
        by_name.insert(hook.name.clone(), hook);
    }
    target.extend(by_name.into_values());
}

fn merge_classes_by_name(target: &mut Vec<ClassEntry>, overrides: Vec<ClassEntry>) {
    let mut by_name = target
        .drain(..)
        .map(|class| (class.name.clone(), class))
        .collect::<BTreeMap<_, _>>();
    for class in overrides {
        by_name.insert(class.name.clone(), class);
    }
    target.extend(by_name.into_values());
}

#[derive(Debug)]
struct TagBlock {
    attrs: String,
    body: String,
    start: usize,
    end: usize,
}

fn tag_blocks(markup: &str, name: &str) -> Vec<TagBlock> {
    let mut blocks = Vec::new();
    let mut search_from = 0;
    while let Some(open) = find_tag(markup, name, search_from) {
        let TagMatchKind::Open { self_closing } = open.kind else {
            search_from = open.end + 1;
            continue;
        };
        search_from = open.end + 1;
        if self_closing {
            continue;
        }

        let body_start = open.end + 1;
        let mut depth = 1usize;
        let mut cursor = body_start;
        while let Some(next) = find_tag(markup, name, cursor) {
            cursor = next.end + 1;
            match next.kind {
                TagMatchKind::Open { self_closing } => {
                    if !self_closing {
                        depth += 1;
                    }
                }
                TagMatchKind::Close => {
                    depth -= 1;
                    if depth == 0 {
                        blocks.push(TagBlock {
                            attrs: markup[open.name_end..open.end]
                                .trim()
                                .trim_end_matches('/')
                                .trim_end()
                                .to_string(),
                            body: markup[body_start..next.start].to_string(),
                            start: open.start,
                            end: next.end + 1,
                        });
                        search_from = next.end + 1;
                        break;
                    }
                }
            }
        }
    }
    blocks
}

fn first_tag_raw(markup: &str, name: &str) -> Option<String> {
    tag_blocks(markup, name)
        .into_iter()
        .next()
        .map(|block| block.body)
}

#[derive(Debug, Clone, Copy)]
struct TagMatch {
    start: usize,
    end: usize,
    name_end: usize,
    kind: TagMatchKind,
}

#[derive(Debug, Clone, Copy)]
enum TagMatchKind {
    Open { self_closing: bool },
    Close,
}

fn find_tag(markup: &str, name: &str, from: usize) -> Option<TagMatch> {
    let mut cursor = from;
    while cursor < markup.len() {
        let relative = markup[cursor..].find('<')?;
        let start = cursor + relative;
        let mut name_start = start + 1;
        let closing = markup.as_bytes().get(name_start).copied() == Some(b'/');
        if closing {
            name_start += 1;
        }
        let name_end = name_start + name.len();
        if tag_name_matches(markup, name_start, name) && tag_name_has_boundary(markup, name_end) {
            let end = find_tag_end(markup, start)?;
            let self_closing = !closing && is_self_closing_tag(&markup[start + 1..end]);
            return Some(TagMatch {
                start,
                end,
                name_end,
                kind: if closing {
                    TagMatchKind::Close
                } else {
                    TagMatchKind::Open { self_closing }
                },
            });
        }
        cursor = start + 1;
    }
    None
}

fn tag_name_matches(markup: &str, start: usize, name: &str) -> bool {
    let Some(candidate) = markup.as_bytes().get(start..start + name.len()) else {
        return false;
    };
    candidate.eq_ignore_ascii_case(name.as_bytes())
}

fn tag_name_has_boundary(markup: &str, index: usize) -> bool {
    matches!(
        markup.as_bytes().get(index).copied(),
        Some(b'>') | Some(b'/') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')
    )
}

fn find_tag_end(markup: &str, start: usize) -> Option<usize> {
    let bytes = markup.as_bytes();
    let mut quote = None;
    for (index, byte) in bytes.iter().enumerate().skip(start + 1) {
        match quote {
            Some(quote_byte) if *byte == quote_byte => quote = None,
            Some(_) => {}
            None if matches!(*byte, b'"' | b'\'') => quote = Some(*byte),
            None if *byte == b'>' => return Some(index),
            None => {}
        }
    }
    None
}

fn is_self_closing_tag(tag_inner: &str) -> bool {
    tag_inner.trim_end().ends_with('/')
}

fn attributes(input: &str) -> BTreeMap<String, String> {
    static ATTR_RE: OnceLock<Regex> = OnceLock::new();
    ATTR_RE
        .get_or_init(|| Regex::new(r#"([A-Za-z_][A-Za-z0-9_-]*)\s*=\s*"([^"]*)""#).unwrap())
        .captures_iter(input)
        .map(|captures| {
            (
                captures.get(1).unwrap().as_str().to_string(),
                decode_entities(captures.get(2).unwrap().as_str()),
            )
        })
        .collect()
}

fn clean_markup(input: &str) -> String {
    static PAGE_TEXT_RE: OnceLock<Regex> = OnceLock::new();
    static PAGE_RE: OnceLock<Regex> = OnceLock::new();
    static TAG_RE: OnceLock<Regex> = OnceLock::new();
    let text = PAGE_TEXT_RE
        .get_or_init(|| Regex::new(r#"(?is)<page\b[^>]*\btext="([^"]+)"[^>]*>.*?</page>"#).unwrap())
        .replace_all(input, "$1")
        .to_string();
    let text = PAGE_RE
        .get_or_init(|| Regex::new(r#"(?is)<page\b[^>]*>(.*?)</page>"#).unwrap())
        .replace_all(&text, "$1")
        .to_string();
    let text = text
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    let text = TAG_RE
        .get_or_init(|| Regex::new(r#"(?is)</?[A-Za-z][^>]*>"#).unwrap())
        .replace_all(&text, "")
        .to_string();
    normalize_markdown(&decode_entities(&text))
}

fn trim_code(input: &str) -> String {
    decode_entities(input)
        .trim_matches('\n')
        .trim_matches('\r')
        .to_string()
}

fn normalize_markdown(input: &str) -> String {
    let mut out = String::new();
    let mut blank_count = 0usize;
    for line in input.replace("\r\n", "\n").replace('\r', "\n").lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                out.push('\n');
            }
        } else {
            blank_count = 0;
            out.push_str(line.trim_start_matches('\t'));
            out.push('\n');
        }
    }
    out.trim().to_string()
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn remove_admonitions(input: &str) -> String {
    ["warning", "note"]
        .into_iter()
        .fold(input.to_string(), |text, tag| remove_tag_blocks(&text, tag))
}

fn remove_tag_blocks(input: &str, tag: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0usize;
    for block in tag_blocks(input, tag) {
        if block.start < cursor {
            continue;
        }
        out.push_str(&input[cursor..block.start]);
        cursor = block.end;
    }
    out.push_str(&input[cursor..]);
    out
}

fn extract_named_sections(markup: &str, tag: &str) -> Vec<String> {
    tag_blocks(markup, tag)
        .into_iter()
        .map(|block| clean_markup(&block.body))
        .filter(|text| !text.is_empty())
        .collect()
}

fn related_pages(markup: &str) -> Vec<String> {
    static PAGE_RE: OnceLock<Regex> = OnceLock::new();
    let mut related = BTreeSet::new();
    for captures in PAGE_RE
        .get_or_init(|| Regex::new(r#"(?is)<page\b[^>]*>(.*?)</page>"#).unwrap())
        .captures_iter(markup)
    {
        let value = clean_markup(captures.get(1).unwrap().as_str());
        if !value.is_empty() {
            related.insert(value);
        }
    }
    related.into_iter().collect()
}

fn summary_from(text: &str) -> String {
    let paragraph = text
        .split("\n\n")
        .map(str::trim)
        .find(|part| !part.is_empty())
        .unwrap_or_default();
    if paragraph.len() <= 240 {
        paragraph.to_string()
    } else {
        format!("{}...", paragraph.chars().take(237).collect::<String>())
    }
}

fn paragraphs(text: &str) -> Vec<String> {
    text.split("\n\n")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create `{}`: {err}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(value)
        .map_err(|err| format!("failed to encode JSON `{}`: {err}", path.display()))?;
    fs::write(path, text).map_err(|err| format!("failed to write `{}`: {err}", path.display()))
}

fn cache_file_name(address: &str) -> String {
    let mut out = String::new();
    for byte in address.bytes() {
        let ch = byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out.push_str(".json");
    out
}

fn encode_path(address: &str) -> String {
    let mut out = String::new();
    for byte in address.bytes() {
        let ch = byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~' | '/' | ':') {
            out.push(ch);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out
}

fn unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

trait IfEmpty {
    fn if_empty(self, fallback: impl FnOnce() -> String) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: impl FnOnce() -> String) -> String {
        if self.trim().is_empty() {
            fallback()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiRealm, ApiSource, PageListItem, WikiPage, source_for};
    use super::{apply_override_files, merge_entries_by_path};
    use super::{
        has_api_markup_block, parse_enum_blocks, parse_function_blocks, parse_realm,
        parse_type_blocks,
    };
    use crate::Args;
    use gmod_api_db::{ApiEntry, ApiKind};
    use std::path::PathBuf;

    fn args() -> Args {
        Args {
            out: PathBuf::from("out.json"),
            coverage_out: None,
            cache_dir: None,
            source_url: "https://wiki.facepunch.com/gmod/~pagelist?format=json".into(),
            base_url: "https://wiki.facepunch.com/gmod".into(),
            limit: None,
            concurrency: 1,
            allow_failures: false,
            offline: false,
            overrides: Vec::new(),
        }
    }

    fn source() -> ApiSource {
        source_for(
            &args(),
            &PageListItem {
                address: "net.Start".into(),
                update_count: Some(1),
                view_count: Some(2),
            },
            &WikiPage {
                title: "Start".into(),
                tags: "function method member realm-client realm-server example".into(),
                address: "net.Start".into(),
                markup: String::new(),
                update_count: Some(3),
            },
        )
    }

    #[test]
    fn parses_function_signature_realm_and_examples() {
        let page = WikiPage {
            title: "Start".into(),
            tags: "function method member realm-client realm-server example".into(),
            address: "net.Start".into(),
            update_count: Some(1),
            markup: r#"
<function name="Start" parent="net" type="libraryfunc">
  <description>Begins a net message.<warning>Must be finished.</warning></description>
  <realm>Shared</realm>
  <args>
    <arg name="messageName" type="string">Message name.</arg>
    <arg name="unreliable" type="boolean" default="false">Use unreliable channel.</arg>
  </args>
  <rets><ret name="" type="boolean">Whether it started.</ret></rets>
</function>
<example><description>Send.</description><code>net.Start("x")</code></example>
"#
            .into(),
        };
        let (entries, hooks) = parse_function_blocks(&page, &source());
        assert!(hooks.is_empty());
        assert_eq!(entries[0].path, "net.Start");
        assert_eq!(entries[0].realm, ApiRealm::Shared);
        assert_eq!(entries[0].signatures[0].parameters.len(), 2);
        assert_eq!(entries[0].examples[0].code, "net.Start(\"x\")");
        assert_eq!(entries[0].warnings[0], "Must be finished.");
    }

    #[test]
    fn parses_panel_functions_as_methods() {
        let page = WikiPage {
            title: "DButton:SetImage".into(),
            tags: "function method member realm-client realm-menu".into(),
            address: "DButton:SetImage".into(),
            update_count: Some(1),
            markup: r#"
<function name="SetImage" parent="DButton" type="panelfunc">
  <description>Sets an image.</description>
  <realm>Client and Menu</realm>
  <args><arg name="img" type="string" default="nil">Image path.</arg></args>
</function>
"#
            .into(),
        };
        let (entries, hooks) = parse_function_blocks(&page, &source());
        assert!(hooks.is_empty());
        assert_eq!(entries[0].path, "DButton:SetImage");
        assert_eq!(entries[0].kind, ApiKind::Method);
        assert_eq!(entries[0].realm, ApiRealm::Client);
    }

    #[test]
    fn parses_hook_callback() {
        let page = WikiPage {
            title: "PlayerInitialSpawn".into(),
            tags: "event member realm-server".into(),
            address: "GM:PlayerInitialSpawn".into(),
            update_count: Some(1),
            markup: r#"
<function name="PlayerInitialSpawn" parent="GM" type="hook">
  <description>Called when a player joins.</description>
  <realm>Server</realm>
  <args><arg name="player" type="Player">The player.</arg></args>
</function>
"#
            .into(),
        };
        let (_, hooks) = parse_function_blocks(&page, &source());
        assert_eq!(hooks[0].name, "PlayerInitialSpawn");
        assert_eq!(hooks[0].gm_path, "GM:PlayerInitialSpawn");
        assert_eq!(hooks[0].callback.parameters[0].ty, "Player");
    }

    #[test]
    fn parses_only_top_level_function_args() {
        let page = WikiPage {
            title: "concommand.Add".into(),
            tags: "function method member realm-client realm-server realm-menu example".into(),
            address: "concommand.Add".into(),
            update_count: Some(1),
            markup: r#"
<function name="Add" parent="concommand" type="libraryfunc">
  <description>Creates a console command.</description>
  <realm>Shared and Menu</realm>
  <args>
    <arg name="name" type="string">The command name.</arg>
    <arg name="callback" type="function">The callback.
      <callback>
        <arg name="ply" type="Player">The player.</arg>
        <arg name="cmd" type="string">The command.</arg>
        <arg name="args" type="table">The arguments.</arg>
        <arg name="argStr" type="string">The raw arguments.</arg>
      </callback>
    </arg>
    <arg name="autoComplete" type="function" default="nil">The autocomplete callback.
      <callback>
        <arg name="cmd" type="string">The command.</arg>
        <arg name="argStr" type="string">The raw arguments.</arg>
        <arg name="args" type="table">The arguments.</arg>
        <ret name="tbl" type="table">The options.</ret>
      </callback>
    </arg>
    <arg name="helpText" type="string" default="nil">The help text.</arg>
    <arg name="flags" type="number{FCVAR}|table<number>" default="0">Console command flags.</arg>
  </args>
</function>
"#
            .into(),
        };

        let (entries, hooks) = parse_function_blocks(&page, &source());
        assert!(hooks.is_empty());
        let signature = &entries[0].signatures[0];
        assert_eq!(
            signature.label,
            "concommand.Add(name, callback, autoComplete = nil, helpText = nil, flags = 0)"
        );
        assert_eq!(
            signature
                .parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>(),
            vec!["name", "callback", "autoComplete", "helpText", "flags"]
        );
        assert_eq!(signature.parameters[4].ty, "number{FCVAR}|table<number>");
        assert_eq!(signature.parameters[4].default.as_deref(), Some("0"));
        let callback = signature.parameters[1]
            .callback
            .as_ref()
            .expect("callback signature");
        assert_eq!(callback.label, "callback(ply, cmd, args, argStr)");
        assert_eq!(
            callback
                .parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>(),
            vec!["ply", "cmd", "args", "argStr"]
        );
        let auto_complete = signature.parameters[2]
            .callback
            .as_ref()
            .expect("autocomplete callback signature");
        assert_eq!(auto_complete.label, "autoComplete(cmd, argStr, args)");
        assert_eq!(auto_complete.returns[0].name, "tbl");
        assert!(
            signature.parameters[1]
                .description
                .contains("The callback.")
        );
        assert!(!signature.parameters[1].description.contains("The player."));
        assert!(
            signature.parameters[2]
                .description
                .contains("The autocomplete callback.")
        );
    }

    #[test]
    fn parses_enums_as_constants() {
        let page = WikiPage {
            title: "TEXT_ALIGN".into(),
            tags: "enum realm-client realm-server".into(),
            address: "Enums/TEXT_ALIGN".into(),
            update_count: Some(1),
            markup: r#"
<enum><realm>Shared</realm><description>Text alignment.</description><items>
<item key="TEXT_ALIGN_LEFT" value="0">Left.</item>
</items></enum>
"#
            .into(),
        };
        let entries = parse_enum_blocks(&page, &source());
        assert!(entries.iter().any(|entry| entry.path == "TEXT_ALIGN"));
        assert!(entries.iter().any(|entry| entry.path == "TEXT_ALIGN_LEFT"));
    }

    #[test]
    fn parses_type_pages() {
        let page = WikiPage {
            title: "net".into(),
            tags: "type".into(),
            address: "net".into(),
            update_count: Some(1),
            markup: r#"<type name="net" category="libraryfunc" is="library"><summary>The net library.</summary></type>"#.into(),
        };
        let entries = parse_type_blocks(&page, &source());
        assert_eq!(entries[0].path, "net");
    }

    #[test]
    fn api_candidate_detection_ignores_literal_angle_words() {
        assert!(has_api_markup_block(
            "<panel><parent>DLabel</parent></panel>"
        ));
        assert!(!has_api_markup_block(
            "Shows panel animation variables: <panelname | blank for all panels>."
        ));
    }

    #[test]
    fn maps_menu_only_realm_without_calling_it_shared() {
        assert_eq!(parse_realm(Some("Menu"), ""), ApiRealm::Menu);
    }

    #[test]
    fn override_entries_replace_official_entries_by_path() {
        let mut entries = vec![ApiEntry {
            path: "net.Start".into(),
            kind: ApiKind::Function,
            realm: ApiRealm::Shared,
            summary: "official".into(),
            description: Vec::new(),
            signatures: Vec::new(),
            warnings: Vec::new(),
            notes: Vec::new(),
            examples: Vec::new(),
            related: Vec::new(),
            official_url: None,
            source: None,
        }];
        merge_entries_by_path(
            &mut entries,
            vec![ApiEntry {
                path: "net.Start".into(),
                kind: ApiKind::Function,
                realm: ApiRealm::Server,
                summary: "override".into(),
                description: Vec::new(),
                signatures: Vec::new(),
                warnings: Vec::new(),
                notes: Vec::new(),
                examples: Vec::new(),
                related: Vec::new(),
                official_url: None,
                source: None,
            }],
        );
        assert_eq!(entries[0].summary, "override");
        assert_eq!(entries[0].realm, ApiRealm::Server);
    }

    #[test]
    fn lightweight_override_file_can_patch_database() {
        let root = std::env::temp_dir().join(format!(
            "lux_gmod_override_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("override dir");
        let override_path = root.join("override.json");
        std::fs::write(
            &override_path,
            r#"{
              "version": "test",
              "entries": [{
                "path": "net.Start",
                "kind": "function",
                "realm": "server",
                "summary": "patched"
              }]
            }"#,
        )
        .expect("write override");

        let mut database = gmod_api_db::ApiDatabase {
            version: "base".into(),
            generated_from: "test".into(),
            generated_at: String::new(),
            source_url: String::new(),
            parser_version: String::new(),
            coverage: None,
            overrides: Vec::new(),
            documents: Vec::new(),
            entries: Vec::new(),
            hooks: Vec::new(),
            classes: Vec::new(),
        };
        apply_override_files(&mut database, &[override_path]).expect("apply override");
        assert_eq!(database.entries[0].summary, "patched");
        assert_eq!(database.overrides[0].version, "test");
        let _ = std::fs::remove_dir_all(root);
    }
}
