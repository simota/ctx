// crates/ctx-where/src/types.rs
//
// Wire types mirroring internal/where/where.go's Suggestion / Match /
// Result / ScoreBreakdown / KeywordSet shapes. JSON tags match Go's
// `json:"..."` annotations byte-for-byte.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[allow(dead_code)]
fn is_empty_string(v: &String) -> bool {
    v.is_empty()
}

fn is_empty_vec<T>(v: &Vec<T>) -> bool {
    v.is_empty()
}

fn is_default_i64(v: &i64) -> bool {
    *v == 0
}

fn is_none_breakdown(v: &Option<ScoreBreakdown>) -> bool {
    v.is_none()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub distance: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    pub line: i64,
    pub column: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "is_empty_vec")]
    pub before: Vec<String>,
    #[serde(default, skip_serializing_if = "is_empty_vec")]
    pub after: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    #[serde(default, skip_serializing_if = "is_default_i64")]
    pub basename: i64,
    #[serde(default, skip_serializing_if = "is_default_i64")]
    pub symbol: i64,
    #[serde(default, skip_serializing_if = "is_default_i64")]
    pub splitname: i64,
    #[serde(default, skip_serializing_if = "is_default_i64")]
    pub path: i64,
    #[serde(default, skip_serializing_if = "is_default_i64")]
    pub content: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Result {
    pub path: String,
    pub score: i64,
    #[serde(default, skip_serializing_if = "is_none_breakdown")]
    pub score_breakdown: Option<ScoreBreakdown>,
    pub reason: String,
    pub matches: Vec<Match>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synonyms_applied: Option<BTreeMap<String, Vec<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expanded_keywords: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeywordSet {
    pub original: String,
    pub synonyms: Vec<String>,
}
