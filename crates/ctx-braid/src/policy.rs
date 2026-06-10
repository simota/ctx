// crates/ctx-braid/src/policy.rs
//
// Port of internal/braid/policy.go MERGED with the subcommand-whitelist
// helpers from braid.go (strandSubcommand / isSupportedSource). Pure
// helpers; the actual MergePaths logic lives in merge.rs.

/// The closed set of subcommand names accepted as a strand source. The
/// MVP intentionally rejects every other subcommand. Mirrors Go's
/// `SupportedSources = []string{"where", "focus", "digest"}`.
pub const SUPPORTED_SOURCES: &[&str] = &["where", "focus", "digest"];

/// strandSubcommand returns the first non-empty token of source,
/// lower-cased. This is the subcommand whitelist check key. Mirrors
/// Go's `strandSubcommand` verbatim (including the leading `ctx` strip).
pub fn strand_subcommand(source: &str) -> String {
    for tok in source.split_whitespace() {
        if tok.is_empty() {
            continue;
        }
        // Strip a leading `ctx` if the user wrote the full command.
        if tok == "ctx" {
            continue;
        }
        return tok.to_ascii_lowercase();
    }
    String::new()
}

pub fn is_supported_source(name: &str) -> bool {
    SUPPORTED_SOURCES.iter().any(|s| *s == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcommand_extracts_first_token() {
        assert_eq!(strand_subcommand("where 'foo' --format json"), "where");
        assert_eq!(strand_subcommand("ctx focus Bar"), "focus");
        assert_eq!(strand_subcommand("  digest --since 7d"), "digest");
        assert_eq!(strand_subcommand(""), "");
    }

    #[test]
    fn supported_source_set() {
        assert!(is_supported_source("where"));
        assert!(is_supported_source("focus"));
        assert!(is_supported_source("digest"));
        assert!(!is_supported_source("bogus"));
        assert!(!is_supported_source(""));
    }
}
