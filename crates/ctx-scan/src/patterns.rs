// crates/ctx-scan/src/patterns.rs
//
// Port of internal/scan/secret.go's `secretPatterns` table plus
// internal/scan/env_patterns.go's `envAssignmentPattern`. Order matches
// the Go source verbatim because secret.go uses `break` after the first
// match per line — divergent ordering would shift `Kind` values for
// lines matching multiple patterns.
//
// REGEX PORTING NOTES
// ===================
// Go's `regexp` is RE2-based with ASCII semantics for `\s`/`\w`/`\d` and
// it does NOT support backreferences. Rust's `regex` defaults to
// Unicode-aware classes. To keep parity:
//
//   * The Go patterns here only use literal character classes (no `\s`
//     except in `env_assignment` where Go's `\s` covers `[\t\n\f\r ]`).
//     For `env_assignment` we use `(?-u:\s)` which restricts `\s` to
//     ASCII whitespace — functionally equivalent to Go's `\s` and
//     UTF-8-safe under Rust's `regex` crate.
//
//   * `\b` in Rust's `regex` is Unicode-aware by default; Go's `\b`
//     fires on ASCII word transitions. For the patterns here the inputs
//     of interest are ASCII identifiers so the boundary check fires at
//     the same positions.
//
//   * `(?i)` is supported on both engines identically.
//
// STORAGE LAYOUT
// ==============
// Rust forbids `Lazy<Regex>` inside `static` slices (E0492: interior
// mutability cannot be hidden behind shared references with extended
// lifetimes). We build the table once at first access via
// `once_cell::sync::Lazy<Vec<SecretPattern>>` and expose a thin
// accessor `secret_patterns()` that returns a `&'static [SecretPattern]`.

use once_cell::sync::Lazy;
use regex::Regex;

/// One row in the secret-pattern table.
#[derive(Debug)]
pub struct SecretPattern {
    pub kind: &'static str,
    pub re: Regex,
    pub severity: &'static str,
}

/// Build the global secret-pattern table. The order here matches
/// internal/scan/secret.go's `secretPatterns` slice plus the
/// env_assignment row appended by env_patterns.go's `init()`.
fn build_patterns() -> Vec<SecretPattern> {
    fn re(pat: &str) -> Regex {
        Regex::new(pat).unwrap_or_else(|e| panic!("compile regex {pat}: {e}"))
    }
    vec![
        SecretPattern {
            kind: "aws_access_key",
            re: re(r"\bAKIA[0-9A-Z]{16}\b"),
            severity: "high",
        },
        SecretPattern {
            kind: "aws_secret_key",
            re: re(
                r#"(?i)aws[_\-\s]*(secret|access)[_\-\s]*key[_\-\s]*[=:][_\-\s]*['"]?([a-zA-Z0-9/+=]{40})['"]?"#,
            ),
            severity: "high",
        },
        SecretPattern {
            kind: "gcp_api_key",
            re: re(r"\bAIza[0-9A-Za-z\-_]{35}\b"),
            severity: "high",
        },
        SecretPattern {
            kind: "gcp_service_account",
            re: re(r#""type":\s*"service_account""#),
            severity: "high",
        },
        SecretPattern {
            kind: "azure_storage_key",
            // No trailing \b: after '=' (non-word) a boundary would REQUIRE a
            // following word char, so a key at EOL or before a quote could
            // never match. The leading \b still anchors the token start.
            re: re(r"\b[a-zA-Z0-9+/=]{86}=="),
            severity: "medium",
        },
        SecretPattern {
            kind: "github_pat",
            re: re(r"\bghp_[A-Za-z0-9]{36,255}\b"),
            severity: "high",
        },
        SecretPattern {
            kind: "github_oauth",
            re: re(r"\bgho_[A-Za-z0-9]{36,255}\b"),
            severity: "high",
        },
        SecretPattern {
            kind: "github_app",
            re: re(r"\bghs_[A-Za-z0-9]{36,255}\b"),
            severity: "high",
        },
        SecretPattern {
            kind: "github_refresh",
            re: re(r"\bghr_[A-Za-z0-9]{36,255}\b"),
            severity: "high",
        },
        SecretPattern {
            kind: "slack_token",
            re: re(r"\bxox[abpr]-[A-Za-z0-9\-]{10,72}\b"),
            severity: "high",
        },
        SecretPattern {
            kind: "slack_webhook",
            re: re(
                r"https://hooks\.slack\.com/services/T[A-Z0-9]+/B[A-Z0-9]+/[A-Za-z0-9]+",
            ),
            severity: "medium",
        },
        SecretPattern {
            kind: "jwt",
            re: re(r"\beyJ[A-Za-z0-9\-_=]+\.eyJ[A-Za-z0-9\-_=]+\.?[A-Za-z0-9\-_.+/=]*\b"),
            severity: "medium",
        },
        SecretPattern {
            kind: "private_key",
            re: re(r"-----BEGIN (RSA |EC |DSA |OPENSSH |PGP |)PRIVATE KEY-----"),
            severity: "high",
        },
        SecretPattern {
            kind: "generic_secret",
            re: re(
                r#"(?i)(api[_\-]?key|secret|token|password)["\s]*[=:]["\s]*['"]([a-zA-Z0-9_\-+/=]{16,})['"]"#,
            ),
            severity: "low",
        },
        // env_patterns.go: appended by init() in the Go source.
        SecretPattern {
            kind: "env_assignment",
            re: re(concat!(
                r"\b[A-Z][A-Z0-9_]{2,}",
                r"(?:API_?KEY|SECRET|TOKEN|PASSWORD|PASSWD|PRIVATE_?KEY|ACCESS_?KEY|AUTH_?KEY)",
                r"[A-Z0-9_]*",
                r"(?-u:\s)*[=:](?-u:\s)*",
                r#"["'`]?[A-Za-z0-9._/+=-]{12,}["'`]?"#,
            )),
            severity: "high",
        },
    ]
}

static SECRET_PATTERN_TABLE: Lazy<Vec<SecretPattern>> = Lazy::new(build_patterns);

/// Returns the ordered secret-pattern table. Thread-safe; the table is
/// constructed once on first access.
pub fn secret_patterns() -> &'static [SecretPattern] {
    &SECRET_PATTERN_TABLE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_aws_access_key() -> String {
        ["AKIA", "IOSFODNN7EXAMPLE"].concat()
    }

    fn sample_openai_env_value() -> String {
        ["sk-", "abcdef0123456789"].concat()
    }

    #[test]
    fn all_patterns_compile() {
        // Sanity check: the table must include the 14 from secret.go
        // PLUS the 1 from env_patterns.go = 15 total.
        assert_eq!(secret_patterns().len(), 15);
    }

    #[test]
    fn aws_access_key_matches() {
        let p = &secret_patterns()[0];
        let key = sample_aws_access_key();
        let line = format!("aws=\"{key}\"");
        let m = p.re.find(&line).unwrap();
        assert_eq!(m.as_str(), key);
    }

    #[test]
    fn env_assignment_matches_openai_key() {
        let p = secret_patterns()
            .iter()
            .find(|p| p.kind == "env_assignment")
            .unwrap();
        let value = sample_openai_env_value();
        let line = format!("OPENAI_API_KEY={value}");
        let m = p.re.find(&line).unwrap();
        assert!(m.as_str().contains(&value));
    }
}
