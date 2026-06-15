use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gmod_api_db::{
    ApiDatabase, ApiEntry, ApiExample, ApiKind, ApiOverrideSource, ApiParameter, ApiRealm,
    ApiReturn, ApiSignature, ApiSource, ClassEntry, CoverageIssue, CoverageManifest, HookEntry,
    OFFICIAL_PAGELIST_URL, OfficialPageRef,
};
use regex::Regex;
use serde::Deserialize;

const PARSER_VERSION: &str = "facepunch-json-markup-v1";
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
                        reason: "not an API documentation page".into(),
                    });
                }
                entries.extend(page.entries);
                hooks.extend(page.hooks);
                for class in page.classes {
                    classes
                        .entry(class.name.clone())
                        .and_modify(|existing| {
                            if existing.summary.is_empty() {
                                existing.summary = class.summary.clone();
                            }
                            existing.methods.extend(class.methods.clone());
                            if existing.official_url.is_none() {
                                existing.official_url = class.official_url.clone();
                            }
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

    let mut database = ApiDatabase {
        version: format!("official-{scraped_at}"),
        generated_from: "Facepunch Garry's Mod Wiki JSON".into(),
        generated_at: scraped_at,
        source_url: args.source_url.clone(),
        parser_version: PARSER_VERSION.into(),
        coverage: Some(coverage.clone()),
        overrides: Vec::new(),
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
         official page JSON payload, converts API markup into gmod-api-db JSON, and\n\
         writes a coverage manifest. Defaults are relative to the lux-lsp workspace.\n\
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
    let mut converted = ConvertedPage {
        address: page.address.clone(),
        page_ref: OfficialPageRef {
            address: page.address.clone(),
            tags: page.tags.clone(),
            update_count: page.update_count.or(list_item.update_count),
            view_count: list_item.view_count,
        },
        api_candidate: is_api_candidate(&page),
        structured: false,
        entries: Vec::new(),
        hooks: Vec::new(),
        classes: Vec::new(),
    };
    let type_entries = parse_type_blocks(&page, &source);
    let type_structured = !type_entries.is_empty();
    converted.entries.extend(type_entries);
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
    converted.structured =
        type_structured || function_structured || enum_structured || structure_structured;

    if converted.api_candidate
        && converted.entries.is_empty()
        && converted.hooks.is_empty()
        && converted.classes.is_empty()
    {
        converted.entries.push(fallback_page_entry(&page, &source));
    }
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
    }) || ["<function", "<enum", "<structure", "<type"]
        .into_iter()
        .any(|needle| page.markup.contains(needle))
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
            "classfunc" => ApiKind::Method,
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
    if address.contains(':') || function_type == "classfunc" || function_type == "hook" {
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
                    Some(ApiParameter {
                        name,
                        ty: attrs.get("type").cloned().unwrap_or_else(|| "any".into()),
                        description: clean_markup(&block.body),
                        optional: default.is_some(),
                        default,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
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
                methods: Vec::new(),
                official_url: Some(format!("{DEFAULT_BASE_URL}/{class_name}")),
                source: None,
            })
            .methods
            .push(entry.clone());
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
}

fn tag_blocks(markup: &str, name: &str) -> Vec<TagBlock> {
    let pattern = format!(
        r#"(?is)<{}\b([^>]*)>(.*?)</{}>"#,
        regex::escape(name),
        regex::escape(name)
    );
    let regex = Regex::new(&pattern).expect("tag regex");
    regex
        .captures_iter(markup)
        .map(|captures| TagBlock {
            attrs: captures
                .get(1)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string(),
            body: captures
                .get(2)
                .map(|m| m.as_str())
                .unwrap_or_default()
                .to_string(),
        })
        .collect()
}

fn first_tag_raw(markup: &str, name: &str) -> Option<String> {
    tag_blocks(markup, name)
        .into_iter()
        .next()
        .map(|block| block.body)
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
    let mut text = input.to_string();
    for tag in ["warning", "note"] {
        let pattern = format!(r#"(?is)<{tag}\b[^>]*>.*?</{tag}>"#);
        text = Regex::new(&pattern)
            .expect("admonition regex")
            .replace_all(&text, "")
            .to_string();
    }
    text
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
    use super::{parse_enum_blocks, parse_function_blocks, parse_realm, parse_type_blocks};
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
