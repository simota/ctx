use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WhereArgs {
    #[serde(default)]
    pub(crate) path: String,
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) limit: i64,
    #[serde(default)]
    pub(crate) format: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SymbolsArgs {
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) page_size: i64,
    #[serde(default)]
    pub(crate) cursor: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BudgetArgs {
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) budget: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackArgs {
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) goal: String,
    #[serde(default)]
    pub(crate) budget: i64,
    #[serde(default)]
    pub(crate) format: String,
    #[serde(default)]
    pub(crate) changed: bool,
    #[serde(default)]
    pub(crate) explain: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SkimArgs {
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) budget: i64,
    #[serde(default)]
    pub(crate) unit: String,
    #[serde(default)]
    pub(crate) lang: String,
    #[serde(default)]
    pub(crate) tier: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FocusArgs {
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) anchor: String,
    #[serde(default)]
    pub(crate) hops: i64,
    #[serde(default)]
    pub(crate) budget: i64,
    #[serde(default)]
    pub(crate) format: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TreeArgs {
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) depth: i64,
    pub(crate) with_tokens: Option<bool>,
    #[serde(default)]
    pub(crate) with_symbols: bool,
    pub(crate) with_git: Option<bool>,
    #[serde(default)]
    pub(crate) page_size: i64,
    #[serde(default)]
    pub(crate) cursor: String,
    pub(crate) since: Option<String>,
    pub(crate) until: Option<String>,
    pub(crate) use_mtime: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DigestArgs {
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) since: String,
    #[serde(default)]
    pub(crate) top: i64,
    #[serde(default)]
    pub(crate) format: String,
    #[serde(default)]
    pub(crate) page_size: i64,
    #[serde(default)]
    pub(crate) cursor: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RootsListArgs {}

#[derive(Clone, Deserialize)]
pub(crate) struct RootsFile {
    #[serde(default)]
    pub(crate) roots: Vec<RootEntry>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct RootEntry {
    pub(crate) name: String,
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) last_opened_at: Option<toml::value::Datetime>,
}

#[derive(Deserialize)]
pub(crate) struct PromptGetParams {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) arguments: BTreeMap<String, String>,
}

#[derive(Deserialize)]
pub(crate) struct ResourceReadParams {
    #[serde(default)]
    pub(crate) uri: String,
}

#[derive(Serialize)]
pub(crate) struct ResourceReadResult {
    pub(crate) contents: Vec<ResourceContent>,
}

#[derive(Serialize)]
pub(crate) struct ResourceContent {
    #[serde(rename = "mimeType")]
    pub(crate) mime_type: &'static str,
    pub(crate) text: String,
    pub(crate) uri: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct McpSymbol {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) line: i32,
}

#[derive(Clone)]
pub(crate) struct BudgetFile {
    pub(crate) path: String,
    pub(crate) abs_path: PathBuf,
    pub(crate) size: i64,
    pub(crate) tokens: i64,
    pub(crate) role: String,
}

#[derive(Clone)]
pub(crate) struct PackCandidate {
    pub(crate) file: PackFile,
    pub(crate) tokens: i64,
    pub(crate) score: i64,
    pub(crate) relevance: String,
    pub(crate) reason: String,
}

#[derive(Clone)]
pub(crate) struct PackFile {
    pub(crate) path: String,
    pub(crate) abs_path: PathBuf,
    pub(crate) size: i64,
    pub(crate) tokens: i64,
    pub(crate) role: String,
    pub(crate) symbols: Vec<ctx_symbols::Symbol>,
}

pub(crate) struct PackPlan {
    pub(crate) high: Vec<PackCandidate>,
    pub(crate) medium: Vec<PackCandidate>,
    pub(crate) skipped: Vec<(String, String)>,
    pub(crate) used: i64,
    pub(crate) budget: i64,
}

#[derive(Clone, Serialize)]
#[allow(non_snake_case)]
pub(crate) struct BudgetItem {
    pub(crate) Path: String,
    pub(crate) Tokens: i64,
    pub(crate) Reason: String,
    pub(crate) Group: String,
}

#[derive(Serialize)]
pub(crate) struct TreeEntry {
    pub(crate) path: String,
    pub(crate) is_dir: bool,
    #[serde(skip_serializing_if = "is_zero")]
    pub(crate) tokens: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) git_status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) symbols: Vec<McpSymbol>,
}

pub(crate) fn is_zero(n: &i64) -> bool {
    *n == 0
}
