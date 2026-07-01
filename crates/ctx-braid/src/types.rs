// crates/ctx-braid/src/types.rs
//
// Wire types mirroring internal/braid's exported surface. JSON tags use
// the **CamelCase** Go field names where the Go side uses default
// encoding/json output (no per-field tag), and explicit snake_case
// where the Go struct has explicit json tags (StrandReport, Result,
// MergedFile).
//
// PolicyKind values are kebab-case: "merge", "prefer-newer",
// "exclude-overlap" — matches Go's PolicyKind = string.

use serde::{Deserialize, Serialize, Serializer};

/// ser_share matches Go's `encoding/json` float64 output: integer-
/// valued floats serialise without the trailing ".0". Without this the
/// goldens diverge — Go emits `"Share":1`, naive Rust would emit
/// `"Share":1.0`.
fn ser_share<S: Serializer>(v: &f64, s: S) -> std::result::Result<S::Ok, S::Error> {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e18 {
        s.serialize_i64(*v as i64)
    } else {
        s.serialize_f64(*v)
    }
}

/// PolicyKind enumerates the per-strand de-duplication strategies.
/// Wire form is the same kebab-case strings the Go side uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PolicyKind {
    #[default]
    #[serde(rename = "merge")]
    Merge,
    #[serde(rename = "prefer-newer")]
    PreferNewer,
    #[serde(rename = "exclude-overlap")]
    ExcludeOverlap,
}

impl PolicyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PolicyKind::Merge => "merge",
            PolicyKind::PreferNewer => "prefer-newer",
            PolicyKind::ExcludeOverlap => "exclude-overlap",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<PolicyKind> {
        match s {
            "merge" => Some(PolicyKind::Merge),
            "prefer-newer" => Some(PolicyKind::PreferNewer),
            "exclude-overlap" => Some(PolicyKind::ExcludeOverlap),
            _ => None,
        }
    }
}

/// Strand is one entry in braid.toml.
///
/// Go field tags use `toml:"..."` (BurntSushi/toml deserialisation) and the
/// JSON output uses Go's default CamelCase. We mirror both.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Strand {
    #[serde(default, rename = "Name", alias = "name")]
    pub name: String,
    #[serde(default, rename = "Source", alias = "source")]
    pub source: String,
    #[serde(
        default,
        rename = "Share",
        alias = "share",
        serialize_with = "ser_share"
    )]
    pub share: f64,
    /// Empty string deserialised here is interpreted as Merge after Validate
    /// normalises (matches Go behaviour).
    #[serde(default, rename = "Policy", alias = "policy")]
    pub policy: PolicyKindOrEmpty,
}

/// PolicyKindOrEmpty mirrors Go's `PolicyKind string` which accepts "" as
/// an unset value that Validate then normalises to "merge". We need this
/// because the TOML parser can produce an empty value when the field is
/// missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PolicyKindOrEmpty(pub Option<PolicyKind>);

impl PolicyKindOrEmpty {
    pub fn unwrap_or_merge(self) -> PolicyKind {
        self.0.unwrap_or(PolicyKind::Merge)
    }

    pub fn is_empty(self) -> bool {
        self.0.is_none()
    }

    pub fn as_str(self) -> &'static str {
        match self.0 {
            Some(p) => p.as_str(),
            None => "",
        }
    }
}

impl Serialize for PolicyKindOrEmpty {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PolicyKindOrEmpty {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        // Accept either string or absent.
        let raw: Option<String> = Option::deserialize(d)?;
        match raw {
            None => Ok(PolicyKindOrEmpty(None)),
            Some(s) if s.is_empty() => Ok(PolicyKindOrEmpty(None)),
            Some(s) => match PolicyKind::from_str_opt(&s) {
                Some(p) => Ok(PolicyKindOrEmpty(Some(p))),
                // Defer the "unknown policy" error to Validate so the error
                // path matches Go (which emits "braid: strand %q: unknown
                // policy ..."). Carry the unrecognised string through.
                None => Err(serde::de::Error::custom(format!(
                    "unknown policy {s:?} (allowed: merge|prefer-newer|exclude-overlap)"
                ))),
            },
        }
    }
}

/// Config is the top-level braid.toml document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, rename = "SchemaVersion", alias = "schema_version")]
    pub schema_version: i64,
    /// TOML `[[strand]]` becomes a `strand` array. When parsed via serde we
    /// expect `strand` (TOML array-of-tables); when parsed via JSON we
    /// accept `Strands` (Go default CamelCase) or `strand`.
    #[serde(default, rename = "Strands", alias = "strand", alias = "strands")]
    pub strands: Vec<Strand>,
}

/// Allocation is the per-strand budget assignment produced from a Config
/// and a global token budget. Mirrors Go's `braid.Allocation`. Go has no
/// json tags so CamelCase ships as-is.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Allocation {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Share", serialize_with = "ser_share")]
    pub share: f64,
    #[serde(rename = "Budget")]
    pub budget: i64,
    #[serde(rename = "Policy")]
    pub policy: PolicyKind,
}

/// StrandSelection mirrors Go's `braid.StrandSelection` (no json tags).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StrandSelection {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Policy")]
    pub policy: PolicyKind,
    #[serde(rename = "Paths")]
    pub paths: Vec<String>,
}

/// MergedFile mirrors Go's `braid.MergedFile` with explicit snake_case tags.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MergedFile {
    #[serde(rename = "path")]
    pub path: String,
    #[serde(rename = "origin")]
    pub origin: String,
}

/// StrandReport mirrors Go's `braid.StrandReport` with explicit snake_case
/// tags. `trim_note` uses omitempty in Go.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StrandReport {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "share", serialize_with = "ser_share")]
    pub share: f64,
    #[serde(rename = "budget")]
    pub budget: i64,
    #[serde(rename = "selected")]
    pub selected: i64,
    #[serde(rename = "tokens")]
    pub tokens: i64,
    #[serde(rename = "policy")]
    pub policy: PolicyKind,
    #[serde(rename = "raw_paths")]
    pub raw_paths: i64,
    #[serde(
        rename = "trim_note",
        skip_serializing_if = "String::is_empty",
        default
    )]
    pub trim_note: String,
}

/// Result mirrors Go's `braid.Result` (the structured outcome of Run).
/// Used by format.rs renderers. Tier 2 #1 ports the renderer helpers, not
/// the Run() orchestrator itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Result {
    #[serde(rename = "file")]
    pub file: String,
    #[serde(rename = "budget")]
    pub budget: i64,
    #[serde(rename = "strands")]
    pub strands: Vec<StrandReport>,
    #[serde(rename = "files")]
    pub files: Vec<MergedFile>,
    #[serde(rename = "tokens_used")]
    pub tokens_used: i64,
    #[serde(rename = "dry_run")]
    pub dry_run: bool,
    #[serde(rename = "pack_bytes", default, skip_serializing_if = "is_zero_i64")]
    pub pack_bytes: i64,
    #[serde(
        rename = "pack_sha256",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub pack_sha256: String,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_kind_roundtrip() {
        for p in [
            PolicyKind::Merge,
            PolicyKind::PreferNewer,
            PolicyKind::ExcludeOverlap,
        ] {
            let s = serde_json::to_string(&p).unwrap();
            let parsed: PolicyKind = serde_json::from_str(&s).unwrap();
            assert_eq!(p, parsed);
        }
    }

    #[test]
    fn policy_empty_deserialises_as_none() {
        // JSON value of "" must round-trip via PolicyKindOrEmpty as None
        // (matches Go's empty PolicyKind default).
        let raw = "\"\"";
        let p: PolicyKindOrEmpty = serde_json::from_str(raw).unwrap();
        assert!(p.is_empty());
        assert_eq!(p.unwrap_or_merge(), PolicyKind::Merge);
    }
}
