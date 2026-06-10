// crates/ctx-pack/src/preset.rs
//
// Port of internal/pack/preset.go::ApplyPreset. The Go side mutates
// a pack.Options struct in place. The Rust side returns a PresetPatch
// payload the Go dispatcher merges onto its own Options. Identifiers
// follow the Go field names so the cgo bridge can do a 1:1 copy.

use crate::types::{PresetName, PresetPatch};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetError {
    Unknown(String),
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PresetError::Unknown(s) => write!(f, "unknown preset {s:?}"),
        }
    }
}

impl std::error::Error for PresetError {}

/// Compute the patch for `name`. Empty name returns Default (no-op).
pub fn apply_preset(name: &str) -> Result<PresetPatch, PresetError> {
    let parsed = match PresetName::parse(name) {
        Some(p) => p,
        None => return Err(PresetError::Unknown(name.to_string())),
    };
    Ok(match parsed {
        PresetName::None => PresetPatch::default(),
        // The variants below explicitly mirror the Go ApplyPreset
        // switch arms in internal/pack/preset.go. Only fields the Go
        // arm assigns appear as Some(...); the rest stay None so the
        // Go-side dispatcher leaves them untouched.
        PresetName::Blog => PresetPatch {
            format: Some("markdown".into()),
            no_warnings: Some(true),
            no_paths: Some(true),
            no_metadata: Some(true),
            frontmatter: Some("mdx".into()),
            plain_file_contents: None,
            explain: None,
        },
        PresetName::Review => PresetPatch {
            format: Some("markdown".into()),
            no_warnings: Some(false),
            no_paths: Some(false),
            no_metadata: Some(false),
            frontmatter: None,
            plain_file_contents: None,
            explain: None,
        },
        PresetName::Debug => PresetPatch {
            format: Some("markdown".into()),
            no_warnings: Some(false),
            no_paths: Some(false),
            no_metadata: Some(false),
            frontmatter: None,
            plain_file_contents: None,
            explain: Some(true),
        },
        PresetName::Llm => PresetPatch {
            format: Some("plain".into()),
            no_warnings: Some(true),
            no_paths: Some(false),
            no_metadata: Some(true),
            frontmatter: None,
            plain_file_contents: Some(true),
            explain: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blog_sets_mdx() {
        let p = apply_preset("blog").unwrap();
        assert_eq!(p.format.as_deref(), Some("markdown"));
        assert_eq!(p.no_warnings, Some(true));
        assert_eq!(p.frontmatter.as_deref(), Some("mdx"));
        // Untouched.
        assert_eq!(p.plain_file_contents, None);
        assert_eq!(p.explain, None);
    }

    #[test]
    fn llm_sets_plain() {
        let p = apply_preset("llm").unwrap();
        assert_eq!(p.format.as_deref(), Some("plain"));
        assert_eq!(p.plain_file_contents, Some(true));
    }

    #[test]
    fn unknown_errors() {
        let r = apply_preset("bogus");
        assert!(r.is_err());
    }

    #[test]
    fn empty_is_noop() {
        let p = apply_preset("").unwrap();
        assert_eq!(p, PresetPatch::default());
    }

    #[test]
    fn debug_sets_explain() {
        let p = apply_preset("debug").unwrap();
        assert_eq!(p.explain, Some(true));
    }

    #[test]
    fn review_keeps_metadata() {
        let p = apply_preset("review").unwrap();
        assert_eq!(p.no_warnings, Some(false));
        assert_eq!(p.no_paths, Some(false));
        assert_eq!(p.no_metadata, Some(false));
    }
}
