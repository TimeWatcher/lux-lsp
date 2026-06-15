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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassEntry {
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub methods: Vec<ApiEntry>,
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
    pub entries: Vec<ApiEntry>,
    #[serde(default)]
    pub hooks: Vec<HookEntry>,
    #[serde(default)]
    pub classes: Vec<ClassEntry>,
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
            entries,
            hooks,
            classes,
        })
    }

    pub fn database(&self) -> &ApiDatabase {
        &self.database
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

    pub fn roots(&self) -> Vec<&ApiEntry> {
        self.entries
            .values()
            .filter(|entry| !entry.path.contains('.') && !entry.path.contains(':'))
            .collect()
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

pub fn entry_markdown(entry: &ApiEntry) -> String {
    let mut out = String::new();
    out.push_str("### ");
    out.push_str(&entry.path);
    out.push_str("\n\n");
    out.push_str(entry.summary.trim());
    out.push_str("\n\n");
    out.push_str("**Kind:** ");
    out.push_str(entry.kind.label());
    out.push_str("  \n**Realm:** ");
    out.push_str(entry.realm.as_str());

    for signature in &entry.signatures {
        out.push_str("\n\n```lua\n");
        out.push_str(&signature.label);
        out.push_str("\n```");
        append_parameters(&mut out, signature);
    }
    append_sections(&mut out, &entry.description, "Details");
    append_sections(&mut out, &entry.warnings, "Warnings");
    append_sections(&mut out, &entry.notes, "Notes");
    append_examples(&mut out, &entry.examples);
    if !entry.related.is_empty() {
        out.push_str("\n\n**Related:** ");
        out.push_str(
            &entry
                .related
                .iter()
                .map(|item| format!("`{item}`"))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    if let Some(url) = &entry.official_url {
        out.push_str("\n\n[Official documentation](");
        out.push_str(url);
        out.push(')');
    }
    out
}

pub fn hook_markdown(hook: &HookEntry) -> String {
    let mut out = String::new();
    out.push_str("### hook: ");
    out.push_str(&hook.name);
    out.push_str("\n\n");
    out.push_str(hook.summary.trim());
    out.push_str("\n\n**Realm:** ");
    out.push_str(hook.realm.as_str());
    out.push_str("\n\n```lua\n");
    out.push_str(&hook.callback.label);
    out.push_str("\n```");
    append_parameters(&mut out, &hook.callback);
    append_sections(&mut out, &hook.description, "Details");
    append_sections(&mut out, &hook.warnings, "Warnings");
    append_sections(&mut out, &hook.notes, "Notes");
    append_examples(&mut out, &hook.examples);
    if let Some(url) = &hook.official_url {
        out.push_str("\n\n[Official documentation](");
        out.push_str(url);
        out.push(')');
    }
    out
}

fn append_parameters(out: &mut String, signature: &ApiSignature) {
    if !signature.parameters.is_empty() {
        out.push_str("\n\n**Parameters**");
        for parameter in &signature.parameters {
            out.push_str("\n- `");
            out.push_str(&parameter.name);
            out.push_str("`: ");
            out.push_str(&parameter.ty);
            if parameter.optional {
                out.push_str(", optional");
            }
            if let Some(default) = &parameter.default {
                out.push_str(", default `");
                out.push_str(default);
                out.push('`');
            }
            if !parameter.description.is_empty() {
                out.push_str(" - ");
                out.push_str(&parameter.description);
            }
        }
    }
    if !signature.returns.is_empty() {
        out.push_str("\n\n**Returns**");
        for return_value in &signature.returns {
            out.push_str("\n- ");
            if !return_value.name.is_empty() {
                out.push('`');
                out.push_str(&return_value.name);
                out.push_str("`: ");
            }
            out.push_str(&return_value.ty);
            if !return_value.description.is_empty() {
                out.push_str(" - ");
                out.push_str(&return_value.description);
            }
        }
    }
}

fn append_sections(out: &mut String, sections: &[String], title: &str) {
    if sections.is_empty() {
        return;
    }
    out.push_str("\n\n**");
    out.push_str(title);
    out.push_str("**");
    for section in sections {
        out.push_str("\n\n");
        out.push_str(section.trim());
    }
}

fn append_examples(out: &mut String, examples: &[ApiExample]) {
    if examples.is_empty() {
        return;
    }
    out.push_str("\n\n**Examples**");
    for example in examples {
        out.push_str("\n\n");
        if !example.title.is_empty() {
            out.push_str(&example.title);
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
    use super::{ApiIndex, ApiRealm, entry_markdown};

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
        assert!(markdown.contains("Parameters"));
        assert!(markdown.contains("Examples"));
        assert!(markdown.contains("Official documentation"));
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
        let coverage = index
            .database()
            .coverage
            .as_ref()
            .expect("bundled database must include coverage metadata");
        assert_eq!(coverage.source_url, crate::OFFICIAL_PAGELIST_URL);
        assert_eq!(coverage.failed_page_count, 0);
        assert!(coverage.official_page_count >= 6000);
        assert!(coverage.api_candidate_count >= 5000);
        assert!(coverage.structured_page_count >= 5000);
        assert!(index.entry("net.Start").is_some());
        assert!(index.entry("net.Broadcast").is_some());
        assert!(index.hook("PlayerInitialSpawn").is_some());
    }
}
