use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const OFFICIAL_PAGELIST_URL: &str = "https://wiki.facepunch.com/gmod/~pagelist?format=json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiRealm {
    Shared,
    Client,
    Server,
    Menu,
}

impl ApiRealm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Client => "client",
            Self::Server => "server",
            Self::Menu => "menu",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKind {
    Global,
    Library,
    Function,
    Hook,
    Class,
    Method,
    Field,
    Enum,
    Constant,
    Struct,
    Panel,
    Page,
}

impl ApiKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Library => "library",
            Self::Function => "function",
            Self::Hook => "hook",
            Self::Class => "class",
            Self::Method => "method",
            Self::Field => "field",
            Self::Enum => "enum",
            Self::Constant => "constant",
            Self::Struct => "struct",
            Self::Panel => "panel",
            Self::Page => "page",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiParameter {
    pub name: String,
    pub ty: String,
    pub description: String,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiReturn {
    #[serde(default)]
    pub name: String,
    pub ty: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiSignature {
    pub label: String,
    #[serde(default)]
    pub parameters: Vec<ApiParameter>,
    #[serde(default)]
    pub returns: Vec<ApiReturn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiExample {
    pub title: String,
    pub language: String,
    pub code: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiSource {
    pub address: String,
    pub url: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub update_count: Option<u64>,
    #[serde(default)]
    pub view_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiEntry {
    pub path: String,
    pub kind: ApiKind,
    pub realm: ApiRealm,
    pub summary: String,
    #[serde(default)]
    pub description: Vec<String>,
    #[serde(default)]
    pub signatures: Vec<ApiSignature>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub examples: Vec<ApiExample>,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub official_url: Option<String>,
    #[serde(default)]
    pub source: Option<ApiSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookEntry {
    pub name: String,
    #[serde(default)]
    pub gm_path: String,
    pub realm: ApiRealm,
    pub summary: String,
    #[serde(default)]
    pub description: Vec<String>,
    pub callback: ApiSignature,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub examples: Vec<ApiExample>,
    #[serde(default)]
    pub official_url: Option<String>,
    #[serde(default)]
    pub source: Option<ApiSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiDocumentStatus {
    Documentation,
    StructuredApi,
    ApiFallback,
}

impl Default for ApiDocumentStatus {
    fn default() -> Self {
        Self::Documentation
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiDocumentPage {
    pub address: String,
    pub title: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub status: ApiDocumentStatus,
    #[serde(default)]
    pub api_candidate: bool,
    #[serde(default)]
    pub structured: bool,
    pub summary: String,
    #[serde(default)]
    pub description: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub examples: Vec<ApiExample>,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub entry_paths: Vec<String>,
    #[serde(default)]
    pub hook_names: Vec<String>,
    #[serde(default)]
    pub class_names: Vec<String>,
    #[serde(default)]
    pub official_url: Option<String>,
    #[serde(default)]
    pub source: Option<ApiSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassEntry {
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub realm: Option<ApiRealm>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub kind: Option<ApiKind>,
    #[serde(default)]
    pub description: Vec<String>,
    #[serde(default)]
    pub methods: Vec<ApiEntry>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub examples: Vec<ApiExample>,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub official_url: Option<String>,
    #[serde(default)]
    pub source: Option<ApiSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficialPageRef {
    pub address: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub update_count: Option<u64>,
    #[serde(default)]
    pub view_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageIssue {
    pub address: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageManifest {
    pub source_url: String,
    pub scraped_at: String,
    pub parser_version: String,
    pub official_page_count: usize,
    #[serde(default)]
    pub document_page_count: usize,
    pub api_candidate_count: usize,
    #[serde(default)]
    pub structured_page_count: usize,
    #[serde(default)]
    pub fallback_page_count: usize,
    pub parsed_page_count: usize,
    pub skipped_page_count: usize,
    pub failed_page_count: usize,
    #[serde(default)]
    pub pages: Vec<OfficialPageRef>,
    #[serde(default)]
    pub skipped_pages: Vec<CoverageIssue>,
    #[serde(default)]
    pub fallback_pages: Vec<CoverageIssue>,
    #[serde(default)]
    pub failed_pages: Vec<CoverageIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiDatabase {
    pub version: String,
    pub generated_from: String,
    #[serde(default)]
    pub generated_at: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub parser_version: String,
    #[serde(default)]
    pub coverage: Option<CoverageManifest>,
    #[serde(default)]
    pub overrides: Vec<ApiOverrideSource>,
    #[serde(default)]
    pub documents: Vec<ApiDocumentPage>,
    #[serde(default)]
    pub entries: Vec<ApiEntry>,
    #[serde(default)]
    pub hooks: Vec<HookEntry>,
    #[serde(default)]
    pub classes: Vec<ClassEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiOverrideSource {
    pub path: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug)]
pub enum ApiDbError {
    Io(io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for ApiDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read GMod API database: {err}"),
            Self::Json(err) => write!(f, "failed to parse GMod API database: {err}"),
        }
    }
}

impl std::error::Error for ApiDbError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiIndex {
    database: ApiDatabase,
    documents: BTreeMap<String, ApiDocumentPage>,
    entries: BTreeMap<String, ApiEntry>,
    hooks: BTreeMap<String, HookEntry>,
    classes: BTreeMap<String, ClassEntry>,
}

impl ApiIndex {
    pub fn bundled() -> Self {
        Self::from_json(bundled_database_text()).expect("bundled GMod API data must be valid")
    }

    pub fn fixture_minimal() -> Self {
        Self::from_json(include_str!("../fixtures/minimal_gmod_api.json"))
            .expect("minimal fixture GMod API data must be valid")
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, ApiDbError> {
        let text = fs::read_to_string(path).map_err(ApiDbError::Io)?;
        let database = serde_json::from_str(&text).map_err(ApiDbError::Json)?;
        Self::from_database(database).map_err(ApiDbError::Json)
    }

    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        let database = serde_json::from_str(text)?;
        Self::from_database(database)
    }

    pub fn from_database(database: ApiDatabase) -> Result<Self, serde_json::Error> {
        let documents = database
            .documents
            .iter()
            .cloned()
            .map(|document| (document.address.clone(), document))
            .collect();
        let mut entries = BTreeMap::new();
        for entry in &database.entries {
            entries.insert(normalize_path(&entry.path), entry.clone());
        }
        for class in &database.classes {
            for method in &class.methods {
                entries.insert(normalize_path(&method.path), method.clone());
            }
        }
        let hooks = database
            .hooks
            .iter()
            .cloned()
            .map(|hook| (hook.name.clone(), hook))
            .collect();
        let classes = database
            .classes
            .iter()
            .cloned()
            .map(|class| (class.name.clone(), class))
            .collect();
        Ok(Self {
            database,
            documents,
            entries,
            hooks,
            classes,
        })
    }

    pub fn database(&self) -> &ApiDatabase {
        &self.database
    }

    pub fn document(&self, address: impl AsRef<str>) -> Option<&ApiDocumentPage> {
        self.documents.get(address.as_ref())
    }

    pub fn documents(&self) -> Vec<&ApiDocumentPage> {
        self.documents.values().collect()
    }

    pub fn entry(&self, path: impl AsRef<str>) -> Option<&ApiEntry> {
        self.entries.get(&normalize_path(path.as_ref()))
    }

    pub fn hook(&self, name: impl AsRef<str>) -> Option<&HookEntry> {
        self.hooks.get(name.as_ref())
    }

    pub fn class(&self, name: impl AsRef<str>) -> Option<&ClassEntry> {
        self.classes.get(name.as_ref())
    }

    pub fn longest_match<'a>(&'a self, path: &[String]) -> Option<&'a ApiEntry> {
        let dotted = path.join(".");
        let colon = path.split_first().and_then(|(head, tail)| {
            (!tail.is_empty()).then(|| format!("{head}:{}", tail.join(".")))
        });
        self.entry(&dotted)
            .or_else(|| colon.as_deref().and_then(|path| self.entry(path)))
            .or_else(|| self.longest_match_text(&dotted))
            .or_else(|| {
                colon
                    .as_deref()
                    .and_then(|path| self.longest_match_text(path))
            })
    }

    pub fn longest_match_text<'a>(&'a self, path: &str) -> Option<&'a ApiEntry> {
        let normalized = normalize_path(path);
        let mut current = normalized.as_str();
        loop {
            if let Some(entry) = self.entry(current) {
                return Some(entry);
            }
            let Some(index) = current.rfind(['.', ':']) else {
                return None;
            };
            current = &current[..index];
        }
    }

    pub fn completions_for_prefix(&self, prefix: &str) -> Vec<&ApiEntry> {
        let normalized = normalize_path(prefix);
        self.entries
            .iter()
            .filter(|(path, _)| path.starts_with(&normalized))
            .map(|(_, entry)| entry)
            .collect()
    }

    pub fn completions_for_member_prefix(&self, prefix: &str) -> Vec<&ApiEntry> {
        let Some(normalized) = normalize_member_prefix(prefix) else {
            return Vec::new();
        };
        self.entries
            .iter()
            .filter(|(path, _)| path.starts_with(&normalized))
            .map(|(_, entry)| entry)
            .collect()
    }

    pub fn roots(&self) -> Vec<&ApiEntry> {
        self.entries
            .values()
            .filter(|entry| !entry.path.contains('.') && !entry.path.contains(':'))
            .collect()
    }

    pub fn methods_for_class(&self, class_name: &str) -> Vec<&ApiEntry> {
        let prefix = format!("{class_name}:");
        self.entries
            .iter()
            .filter(|(path, entry)| path.starts_with(&prefix) && entry.kind == ApiKind::Method)
            .map(|(_, entry)| entry)
            .collect()
    }

    pub fn methods_for_class_and_bases(&self, class_name: &str) -> Vec<&ApiEntry> {
        let mut seen = BTreeMap::<String, ()>::new();
        let mut methods = Vec::new();
        for class_name in self.class_lineage_names(class_name) {
            for method in self.methods_for_class(&class_name) {
                let method_name = method
                    .path
                    .rsplit(':')
                    .next()
                    .unwrap_or(method.path.as_str())
                    .to_string();
                if seen.insert(method_name, ()).is_none() {
                    methods.push(method);
                }
            }
        }
        methods
    }

    pub fn method_for_class_or_base(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Option<&ApiEntry> {
        let method_name = method_name.rsplit(':').next().unwrap_or(method_name);
        for class_name in self.class_lineage_names(class_name) {
            let path = format!("{class_name}:{method_name}");
            if let Some(entry) = self.entry(path)
                && entry.kind == ApiKind::Method
            {
                return Some(entry);
            }
        }
        None
    }

    pub fn class_names(&self) -> Vec<&str> {
        self.classes.keys().map(String::as_str).collect()
    }

    fn class_lineage_names(&self, class_name: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = BTreeMap::<String, ()>::new();
        let mut current = class_name.trim().to_string();
        while !current.is_empty() && seen.insert(current.clone(), ()).is_none() {
            names.push(current.clone());
            let Some(class) = self.class(&current) else {
                break;
            };
            let Some(parent) = class
                .parent
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                break;
            };
            current = parent.to_string();
        }
        names
    }
}

pub fn path_parts(path: impl AsRef<str>) -> Vec<String> {
    normalize_path(path.as_ref())
        .split('.')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn normalize_path(path: &str) -> String {
    let path = path.replace(':', ":");
    path.split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

fn normalize_member_prefix(prefix: &str) -> Option<String> {
    let prefix = prefix.trim();
    let delimiter = prefix
        .chars()
        .last()
        .filter(|delimiter| matches!(delimiter, '.' | ':'))?;
    let base = &prefix[..prefix.len() - delimiter.len_utf8()];
    let base = normalize_path(base);
    if base.is_empty() {
        None
    } else {
        Some(format!("{base}{delimiter}"))
    }
}

pub fn entry_markdown(entry: &ApiEntry) -> String {
    let mut out = String::new();
    out.push_str(&realm_badge(entry.realm));
    if api_entry_is_deprecated(entry) {
        out.push_str(" `DEPRECATED`");
    }
    out.push_str("\n\n## ");
    out.push_str(&entry.path);
    out.push_str("\n\n");
    append_summary(&mut out, &entry.summary);
    out.push_str("\n\n");
    out.push_str("| Kind | Realm |\n| --- | --- |\n| `");
    out.push_str(entry.kind.label());
    out.push_str("` | ");
    out.push_str(&realm_inline(entry.realm));
    out.push_str(" |");

    for signature in &entry.signatures {
        out.push_str("\n\n---\n\n**Signature**\n\n```lua\n");
        out.push_str(&signature.label);
        out.push_str("\n```");
        append_parameters(&mut out, signature);
    }
    append_sections(&mut out, &entry.description, "Details");
    append_sections(&mut out, &entry.warnings, "Warnings");
    append_sections(&mut out, &entry.notes, "Notes");
    append_examples(&mut out, &entry.examples);
    append_related(&mut out, &entry.related);
    append_links(
        &mut out,
        entry.official_url.as_deref(),
        entry.source.as_ref(),
    );
    out
}

pub fn hook_markdown(hook: &HookEntry) -> String {
    let mut out = String::new();
    out.push_str(&realm_badge(hook.realm));
    out.push_str(" `HOOK`\n\n## hook: ");
    out.push_str(&hook.name);
    out.push_str("\n\n");
    append_summary(&mut out, &hook.summary);
    out.push_str("\n\n| Kind | Realm |\n| --- | --- |\n| `hook` | ");
    out.push_str(&realm_inline(hook.realm));
    out.push_str(" |");
    out.push_str("\n\n---\n\n**Callback**\n\n```lua\n");
    out.push_str(&hook.callback.label);
    out.push_str("\n```");
    append_parameters(&mut out, &hook.callback);
    append_sections(&mut out, &hook.description, "Details");
    append_sections(&mut out, &hook.warnings, "Warnings");
    append_sections(&mut out, &hook.notes, "Notes");
    append_examples(&mut out, &hook.examples);
    append_links(&mut out, hook.official_url.as_deref(), hook.source.as_ref());
    out
}

fn append_parameters(out: &mut String, signature: &ApiSignature) {
    if !signature.parameters.is_empty() {
        out.push_str("\n\n**Parameters**\n\n");
        out.push_str("| Name | Type | Notes |\n| --- | --- | --- |");
        for parameter in &signature.parameters {
            out.push_str("\n| `");
            out.push_str(&parameter.name);
            out.push_str("` | `");
            out.push_str(&parameter.ty);
            out.push_str("` | ");
            let mut notes = Vec::new();
            if parameter.optional {
                notes.push("optional".to_string());
            }
            if let Some(default) = &parameter.default {
                notes.push(format!("default `{default}`"));
            }
            if !parameter.description.is_empty() {
                notes.push(clean_table_cell(&parameter.description));
            }
            out.push_str(&notes.join("; "));
            out.push_str(" |");
        }
    }
    if !signature.returns.is_empty() {
        out.push_str("\n\n**Returns**\n\n");
        out.push_str("| Type | Name | Description |\n| --- | --- | --- |");
        for return_value in &signature.returns {
            out.push_str("\n| ");
            out.push_str(&type_icon(&return_value.ty));
            out.push(' ');
            out.push('`');
            out.push_str(&return_value.ty);
            out.push_str("` | ");
            if !return_value.name.is_empty() {
                out.push('`');
                out.push_str(&return_value.name);
                out.push('`');
            }
            out.push_str(" | ");
            if !return_value.description.is_empty() {
                out.push_str(&clean_table_cell(&return_value.description));
            }
            out.push_str(" |");
        }
    }
}

fn append_sections(out: &mut String, sections: &[String], title: &str) {
    if sections.is_empty() {
        return;
    }
    out.push_str("\n\n---\n\n**");
    out.push_str(title);
    out.push_str("**");
    for section in sections {
        out.push_str("\n\n");
        match title {
            "Warnings" => {
                out.push_str("> **Warning:** ");
                out.push_str(section.trim());
            }
            "Notes" => {
                out.push_str("> **Note:** ");
                out.push_str(section.trim());
            }
            _ => out.push_str(section.trim()),
        }
    }
}

fn append_examples(out: &mut String, examples: &[ApiExample]) {
    if examples.is_empty() {
        return;
    }
    out.push_str("\n\n---\n\n**Examples**");
    for example in examples {
        out.push_str("\n\n");
        if !example.title.is_empty() {
            out.push_str("**");
            out.push_str(&example.title);
            out.push_str("**");
            out.push_str("\n\n");
        }
        if !example.description.is_empty() {
            out.push_str(example.description.trim());
            out.push_str("\n\n");
        }
        out.push_str("```");
        out.push_str(&example.language);
        out.push('\n');
        out.push_str(example.code.trim());
        out.push_str("\n```");
        if !example.output.is_empty() {
            out.push_str("\n\nOutput:\n\n```text\n");
            out.push_str(example.output.trim());
            out.push_str("\n```");
        }
    }
}

fn append_summary(out: &mut String, summary: &str) {
    let summary = summary.trim();
    if !summary.is_empty() {
        out.push_str(summary);
    }
}

fn append_related(out: &mut String, related: &[String]) {
    if related.is_empty() {
        return;
    }
    out.push_str("\n\n---\n\n**Related**\n\n");
    out.push_str(
        &related
            .iter()
            .map(|item| format!("`{item}`"))
            .collect::<Vec<_>>()
            .join("  "),
    );
}

fn append_links(out: &mut String, official_url: Option<&str>, source: Option<&ApiSource>) {
    let mut links = Vec::new();
    if let Some(url) = official_url {
        links.push(format!("[📘 Official documentation]({url})"));
    }
    if let Some(source) = source {
        links.push(format!("[🔎 View source]({})", source.url));
    }
    if !links.is_empty() {
        out.push_str("\n\n---\n\n");
        out.push_str(&links.join(" | "));
    }
}

fn realm_badge(realm: ApiRealm) -> String {
    let (id, label) = match realm {
        ApiRealm::Client => ("c", "CLIENT"),
        ApiRealm::Server => ("s", "SERVER"),
        ApiRealm::Shared => ("cs", "CLIENT/SERVER"),
        ApiRealm::Menu => ("m", "MENU"),
    };
    format!("![{label}](lux-resource://realm/{id}.svg) **{label}**")
}

fn realm_inline(realm: ApiRealm) -> String {
    match realm {
        ApiRealm::Client => "`client`",
        ApiRealm::Server => "`server`",
        ApiRealm::Shared => "`shared`",
        ApiRealm::Menu => "`menu`",
    }
    .to_string()
}

fn type_icon(ty: &str) -> &'static str {
    match ty.to_ascii_lowercase().as_str() {
        "number" => "#",
        "string" => "\"",
        "bool" | "boolean" => "✓",
        "function" => "ƒ",
        "table" | "userdata" => "{}",
        "entity" | "ent" | "player" | "ply" | "panel" => "◇",
        "nil" => "∅",
        _ => "•",
    }
}

fn clean_table_cell(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("\">")
        .trim_start_matches('>')
        .trim()
        .replace('\n', "<br>")
        .replace('|', "\\|")
}

fn api_entry_is_deprecated(entry: &ApiEntry) -> bool {
    let contains_deprecated = |value: &str| value.to_ascii_lowercase().contains("deprecated");
    contains_deprecated(&entry.summary)
        || entry
            .warnings
            .iter()
            .any(|value| contains_deprecated(value))
        || entry.notes.iter().any(|value| contains_deprecated(value))
        || entry
            .source
            .as_ref()
            .is_some_and(|source| contains_deprecated(&source.tags))
}

fn bundled_database_text() -> &'static str {
    let text = include_str!("../data/generated/gmod_api.json");
    if text.trim().is_empty() {
        panic!(
            "bundled GMod API database is empty; run `cargo run -p gmod-api-update -- --out crates/gmod-api-db/data/generated/gmod_api.json --coverage-out crates/gmod-api-db/data/generated/coverage_manifest.json`"
        );
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{ApiDocumentStatus, ApiIndex, ApiRealm, entry_markdown};

    #[test]
    fn longest_path_lookup_prefers_member_data() {
        let index = ApiIndex::fixture_minimal();
        let entry = index
            .longest_match(&["net".into(), "Broadcast".into()])
            .expect("net.Broadcast");
        assert_eq!(entry.path, "net.Broadcast");
        assert_eq!(entry.realm, ApiRealm::Server);
    }

    #[test]
    fn markdown_contains_documentation_level_sections() {
        let index = ApiIndex::fixture_minimal();
        let entry = index.entry("net.Start").expect("net.Start");
        let markdown = entry_markdown(entry);
        assert!(
            markdown.contains("lux-resource://realm/cs.svg"),
            "{markdown}"
        );
        assert!(!markdown.contains("data:image/svg+xml"), "{markdown}");
        assert!(markdown.contains("Parameters"));
        assert!(markdown.contains("Examples"));
        assert!(markdown.contains("Official documentation"));
        assert!(!markdown.contains("$("), "{markdown}");
    }

    #[test]
    fn markdown_sanitizes_table_cells_for_hover_renderer() {
        let index = ApiIndex::bundled();
        let entry = index.entry("player.GetAll").expect("player.GetAll");
        let markdown = entry_markdown(entry);
        assert!(!markdown.contains("$("), "{markdown}");
        assert!(!markdown.contains("\">All Players"), "{markdown}");
        assert!(
            markdown.contains("All Players currently in the server."),
            "{markdown}"
        );
    }

    #[test]
    fn member_prefix_completion_keeps_delimiter_boundary() {
        let index = ApiIndex::bundled();
        let paths = index
            .completions_for_member_prefix("player.")
            .into_iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();

        assert!(paths.iter().any(|path| *path == "player.GetAll"), "{paths:#?}");
        assert!(!paths.iter().any(|path| *path == "player"), "{paths:#?}");
        assert!(
            !paths.iter().any(|path| *path == "player_manager"),
            "{paths:#?}"
        );
        assert!(
            paths.iter().all(|path| path.starts_with("player.")),
            "{paths:#?}"
        );
    }

    #[test]
    fn markdown_links_to_source_when_available() {
        let index = ApiIndex::bundled();
        let entry = index
            .database()
            .entries
            .iter()
            .find(|entry| entry.source.is_some())
            .expect("bundled API entry with source metadata");
        let markdown = entry_markdown(entry);
        assert!(markdown.contains("View source"), "{markdown}");
    }

    #[test]
    fn hooks_have_callback_signatures() {
        let index = ApiIndex::fixture_minimal();
        let hook = index.hook("PlayerInitialSpawn").expect("hook");
        assert!(hook.callback.label.contains("PlayerInitialSpawn"));
        assert_eq!(hook.callback.parameters[0].name, "ply");
    }

    #[test]
    fn bundled_database_is_generated_from_official_coverage_manifest() {
        let index = ApiIndex::bundled();
        let database = index.database();
        let coverage = index
            .database()
            .coverage
            .as_ref()
            .expect("bundled database must include coverage metadata");
        assert_eq!(
            database.generated_from, "Facepunch Garry's Mod Wiki JSON",
            "the bundled database must be generated from official Facepunch JSON"
        );
        assert_eq!(coverage.source_url, crate::OFFICIAL_PAGELIST_URL);
        assert_eq!(coverage.failed_page_count, 0);
        assert_eq!(
            coverage.fallback_page_count, 0,
            "API candidate pages must not be silently downgraded to fallback docs"
        );
        assert!(coverage.official_page_count >= 6000);
        assert_eq!(coverage.pages.len(), coverage.official_page_count);
        assert_eq!(coverage.document_page_count, coverage.official_page_count);
        assert_eq!(index.documents().len(), coverage.official_page_count);
        assert!(coverage.api_candidate_count >= 5000);
        assert_eq!(
            coverage.structured_page_count, coverage.api_candidate_count,
            "every official API candidate page must be structurally converted"
        );
        assert_eq!(
            coverage.skipped_page_count + coverage.api_candidate_count,
            coverage.official_page_count,
            "coverage accounting must explain every official page"
        );
        for page in &coverage.pages {
            assert!(
                index.document(&page.address).is_some(),
                "missing official document page `{}`",
                page.address
            );
        }
        for document in index.documents() {
            assert!(
                document.official_url.is_some(),
                "official document page `{}` is missing its source URL",
                document.address
            );
            assert!(
                document.source.is_some(),
                "official document page `{}` is missing source metadata",
                document.address
            );
            if document.api_candidate {
                assert!(
                    document.structured,
                    "official API candidate `{}` was not structurally converted",
                    document.address
                );
                assert_eq!(document.status, ApiDocumentStatus::StructuredApi);
                assert!(
                    !document.entry_paths.is_empty()
                        || !document.hook_names.is_empty()
                        || !document.class_names.is_empty(),
                    "structured API page `{}` produced no indexed symbols",
                    document.address
                );
            } else {
                assert_eq!(document.status, ApiDocumentStatus::Documentation);
            }
        }
        assert!(index.entry("net.Start").is_some());
        assert!(index.entry("net.Broadcast").is_some());
        assert!(index.hook("PlayerInitialSpawn").is_some());
        assert!(!index.methods_for_class("Player").is_empty());
        assert_eq!(
            index
                .class("DButton")
                .and_then(|class| class.parent.as_deref()),
            Some("DLabel")
        );
        assert_eq!(
            index
                .method_for_class_or_base("DButton", "SetSize")
                .map(|entry| entry.path.as_str()),
            Some("Panel:SetSize")
        );
    }
}
