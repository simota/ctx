// crates/ctx-braid/src/config.rs
//
// Port of internal/braid/config.go — TOML schema load + validate +
// SortedStrandNames.
//
// The Go side uses BurntSushi/toml; we use the `toml` crate which
// produces byte-equivalent parsing for the schema used by braid
// (top-level keys + an array of `[[strand]]` tables). The schema is
// simple enough that no quirks need normalisation.

use serde::Deserialize;

use crate::policy::{is_supported_source, strand_subcommand};
use crate::types::{Config, PolicyKind, Strand};

/// SchemaVersion is the current braid.toml schema. Bump when the layout
/// changes in a non-additive way. Mirrors Go's `SchemaVersion = 1`.
pub const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigError {
    Read(String),
    Parse(String),
    SchemaTooNew(i64),
    NoStrands,
    StrandNameRequired(usize),
    DuplicateStrandName(String),
    SourceRequired(String),
    ShareOutOfRange { strand: String, share: f64 },
    UnknownPolicy { strand: String, policy: String },
    UnsupportedSource { strand: String, source: String },
    UnclosedQuote(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Read(s) => write!(f, "{s}"),
            ConfigError::Parse(s) => write!(f, "parse braid toml: {s}"),
            ConfigError::SchemaTooNew(v) => write!(
                f,
                "braid: schema_version {v} not supported (max {SCHEMA_VERSION})"
            ),
            ConfigError::NoStrands => write!(f, "braid: at least one [[strand]] is required"),
            ConfigError::StrandNameRequired(i) => {
                write!(f, "braid: strand[{i}].name is required")
            }
            ConfigError::DuplicateStrandName(name) => {
                write!(f, "braid: duplicate strand name \"{name}\"")
            }
            ConfigError::SourceRequired(name) => {
                write!(f, "braid: strand \"{name}\": source is required")
            }
            ConfigError::ShareOutOfRange { strand, share } => write!(
                f,
                "braid: strand \"{strand}\": share must be in (0, 1], got {share}"
            ),
            ConfigError::UnknownPolicy { strand, policy } => write!(
                f,
                "braid: strand \"{strand}\": unknown policy \"{policy}\" (allowed: merge|prefer-newer|exclude-overlap)"
            ),
            ConfigError::UnsupportedSource { strand, source } => write!(
                f,
                "braid: strand \"{strand}\": unsupported source \"{source}\" (allowed: where|focus|digest)"
            ),
            ConfigError::UnclosedQuote(s) => write!(f, "braid: {s}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Internal parse shape — the TOML deserialiser is more permissive on
/// unknown-policy values than serde's strict enum. We collect the raw
/// policy string here and apply the kebab-case check explicitly during
/// Validate so the error message matches Go.
#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    #[serde(default, alias = "schema_version")]
    schema_version: i64,
    #[serde(default, alias = "strand")]
    strand: Vec<RawStrand>,
}

#[derive(Debug, Default, Deserialize)]
struct RawStrand {
    #[serde(default)]
    name: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    share: f64,
    #[serde(default)]
    policy: String,
}

/// loadFromFile reads a braid.toml document from disk and validates it.
pub fn load_from_file(path: &std::path::Path) -> Result<Config, ConfigError> {
    let data = std::fs::read(path)
        .map_err(|e| ConfigError::Read(format!("read braid file {}: {e}", path.display())))?;
    load(&data)
}

/// load parses a braid.toml document from bytes and validates it.
pub fn load(data: &[u8]) -> Result<Config, ConfigError> {
    let raw_text = std::str::from_utf8(data)
        .map_err(|e| ConfigError::Parse(format!("invalid utf-8: {e}")))?;
    let raw: RawConfig = match toml::from_str(raw_text) {
        Ok(c) => c,
        Err(e) => return Err(ConfigError::Parse(e.message().to_string())),
    };

    let mut cfg = Config {
        schema_version: raw.schema_version,
        strands: raw
            .strand
            .into_iter()
            .map(|rs| Strand {
                name: rs.name,
                source: rs.source,
                share: rs.share,
                policy: crate::types::PolicyKindOrEmpty(if rs.policy.is_empty() {
                    None
                } else {
                    PolicyKind::from_str_opt(&rs.policy).or(Some(PolicyKind::Merge))
                }),
            })
            .collect(),
    };

    // Re-attach the raw policy strings on parsed strands when they had an
    // unknown value so Validate can emit the Go-compatible error. We do this
    // by re-parsing the file (cheap relative to the FFI floor we're about
    // to cross) when validate fails its policy switch.
    if cfg.schema_version == 0 {
        cfg.schema_version = SCHEMA_VERSION;
    }

    // Pre-validate unknown policy strings while we still have raw access.
    // Use a side-channel pass: re-parse just to recover policy strings.
    let raw_again: RawConfig = toml::from_str(raw_text)
        .map_err(|e| ConfigError::Parse(e.message().to_string()))?;
    for rs in raw_again.strand.iter() {
        if !rs.policy.is_empty() && PolicyKind::from_str_opt(&rs.policy).is_none() {
            // Use the strand name; if missing, the name-required error in
            // validate will surface first below.
            let strand_name = if rs.name.is_empty() {
                "<unnamed>".to_string()
            } else {
                rs.name.clone()
            };
            return Err(ConfigError::UnknownPolicy {
                strand: strand_name,
                policy: rs.policy.clone(),
            });
        }
    }

    validate(&mut cfg)?;
    Ok(cfg)
}

/// validate checks structural invariants on cfg and normalises empty
/// policy fields to PolicyMerge. Mirrors Go's `Validate` verbatim.
pub fn validate(cfg: &mut Config) -> Result<(), ConfigError> {
    if cfg.schema_version > SCHEMA_VERSION {
        return Err(ConfigError::SchemaTooNew(cfg.schema_version));
    }
    if cfg.strands.is_empty() {
        return Err(ConfigError::NoStrands);
    }

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for i in 0..cfg.strands.len() {
        let name_trimmed = cfg.strands[i].name.trim().to_string();
        if name_trimmed.is_empty() {
            return Err(ConfigError::StrandNameRequired(i));
        }
        if seen.contains(&cfg.strands[i].name) {
            return Err(ConfigError::DuplicateStrandName(cfg.strands[i].name.clone()));
        }
        seen.insert(cfg.strands[i].name.clone());

        if cfg.strands[i].source.trim().is_empty() {
            return Err(ConfigError::SourceRequired(cfg.strands[i].name.clone()));
        }

        let share = cfg.strands[i].share;
        if !(share > 0.0 && share <= 1.0) {
            return Err(ConfigError::ShareOutOfRange {
                strand: cfg.strands[i].name.clone(),
                share,
            });
        }

        if cfg.strands[i].policy.is_empty() {
            cfg.strands[i].policy = crate::types::PolicyKindOrEmpty(Some(PolicyKind::Merge));
        }
        // Policy is already a typed enum-or-empty; the unknown-string case
        // was rejected during load().

        let sub = strand_subcommand(&cfg.strands[i].source);
        if !is_supported_source(&sub) {
            return Err(ConfigError::UnsupportedSource {
                strand: cfg.strands[i].name.clone(),
                source: sub,
            });
        }
    }
    Ok(())
}

/// sortedStrandNames returns strand names in declaration order then
/// sorted (matches Go's `SortedStrandNames`, which despite the name first
/// collects in declaration order then calls sort.Strings).
pub fn sorted_strand_names(cfg: &Config) -> Vec<String> {
    let mut out: Vec<String> = cfg.strands.iter().map(|s| s.name.clone()).collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_valid_toml() {
        let data = br#"schema_version = 1

[[strand]]
name = "a"
source = "where 'foo'"
share = 0.4

[[strand]]
name = "b"
source = "focus Bar"
share = 0.3
policy = "prefer-newer"

[[strand]]
name = "c"
source = "digest --since 7d"
share = 0.3
"#;
        let cfg = load(data).unwrap();
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(cfg.strands.len(), 3);
        assert_eq!(cfg.strands[0].policy.unwrap_or_merge(), PolicyKind::Merge);
        assert_eq!(
            cfg.strands[1].policy.unwrap_or_merge(),
            PolicyKind::PreferNewer
        );
    }

    #[test]
    fn validate_rejects_duplicate_names() {
        let mut cfg = Config {
            schema_version: 1,
            strands: vec![
                Strand {
                    name: "x".into(),
                    source: "where 'a'".into(),
                    share: 0.5,
                    policy: Default::default(),
                },
                Strand {
                    name: "x".into(),
                    source: "focus B".into(),
                    share: 0.5,
                    policy: Default::default(),
                },
            ],
        };
        match validate(&mut cfg).unwrap_err() {
            ConfigError::DuplicateStrandName(s) => assert_eq!(s, "x"),
            e => panic!("expected duplicate-name, got {e:?}"),
        }
    }

    #[test]
    fn validate_rejects_share_zero() {
        let mut cfg = Config {
            schema_version: 1,
            strands: vec![Strand {
                name: "x".into(),
                source: "where 'a'".into(),
                share: 0.0,
                policy: Default::default(),
            }],
        };
        match validate(&mut cfg).unwrap_err() {
            ConfigError::ShareOutOfRange { strand, .. } => assert_eq!(strand, "x"),
            e => panic!("expected share-out-of-range, got {e:?}"),
        }
    }

    #[test]
    fn load_rejects_invalid_source() {
        let data = br#"schema_version = 1

[[strand]]
name = "bogus"
source = "unknown-subcommand --flag"
share = 0.5
"#;
        let err = load(data).unwrap_err();
        match err {
            ConfigError::UnsupportedSource { source, .. } => {
                assert_eq!(source, "unknown-subcommand");
            }
            other => panic!("expected unsupported-source, got {other:?}"),
        }
    }

    #[test]
    fn load_rejects_unknown_policy() {
        let data = br#"schema_version = 1

[[strand]]
name = "a"
source = "where 'foo'"
share = 0.5
policy = "weird"
"#;
        let err = load(data).unwrap_err();
        match err {
            ConfigError::UnknownPolicy { strand, policy } => {
                assert_eq!(strand, "a");
                assert_eq!(policy, "weird");
            }
            other => panic!("expected unknown-policy, got {other:?}"),
        }
    }

    #[test]
    fn sorted_strand_names_alpha() {
        let cfg = Config {
            schema_version: 1,
            strands: vec![
                Strand {
                    name: "zebra".into(),
                    source: "where 'x'".into(),
                    share: 0.5,
                    policy: Default::default(),
                },
                Strand {
                    name: "alpha".into(),
                    source: "where 'y'".into(),
                    share: 0.5,
                    policy: Default::default(),
                },
            ],
        };
        let names = sorted_strand_names(&cfg);
        assert_eq!(names, vec!["alpha".to_string(), "zebra".into()]);
    }
}
