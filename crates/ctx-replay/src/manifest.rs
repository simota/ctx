// crates/ctx-replay/src/manifest.rs
//
// Port of internal/replay/manifest.go — BuildManifest constructor.

use std::fs;

use sha2::{Digest, Sha256};

use crate::types::{Entry, Manifest, Skipped, SCHEMA_VERSION};

/// EntryInput mirrors `replay.EntryInput`.
#[derive(Debug, Clone, Default)]
pub struct EntryInput {
    pub path: String,
    pub abs_path: String,
    pub tokens: i64,
    pub relevance: String,
    pub score: i64,
    pub reason: String,
}

/// SkippedInput mirrors `replay.SkippedInput`.
#[derive(Debug, Clone, Default)]
pub struct SkippedInput {
    pub path: String,
    pub reason: String,
}

/// BuildInput mirrors `replay.BuildInput`.
#[derive(Debug, Clone, Default)]
pub struct BuildInput {
    pub id: String,
    /// RFC3339 string. The Go side defaults to time.Now().UTC() when empty;
    /// in the Rust port we treat empty as "caller-handles", which matches
    /// the dispatcher behaviour (Go sets the timestamp before crossing FFI).
    pub created_at: String,
    pub ctx_version: String,
    pub goal: String,
    pub budget: i64,
    pub used: i64,
    pub root: String,
    pub preset: String,
    pub format: String,
    pub out_sha256: String,
    pub included: Vec<EntryInput>,
    pub skipped: Vec<SkippedInput>,
}

/// Mirrors `replay.BuildManifest`. Computes a SHA-256 over each
/// included file's bytes; an unreadable file aborts the whole build.
pub fn build_manifest(input: BuildInput) -> std::io::Result<Manifest> {
    let mut m = Manifest {
        schema_version: SCHEMA_VERSION,
        id: input.id,
        created_at: input.created_at,
        ctx_version: input.ctx_version,
        goal: input.goal,
        budget: input.budget,
        used: input.used,
        root: input.root,
        preset: input.preset,
        format: input.format,
        out_sha256: input.out_sha256,
        entries: Vec::with_capacity(input.included.len()),
        skipped: Vec::with_capacity(input.skipped.len()),
    };
    for e in input.included {
        let sum = hash_file(&e.abs_path)?;
        m.entries.push(Entry {
            path: e.path,
            sha256: sum,
            tokens: e.tokens,
            relevance: e.relevance,
            score: e.score,
            reason: e.reason,
        });
    }
    for s in input.skipped {
        m.skipped.push(Skipped {
            path: s.path,
            reason: s.reason,
        });
    }
    Ok(m)
}

fn hash_file(path: &str) -> std::io::Result<String> {
    let data = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    Ok(hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_manifest_hashes_files() {
        let tmp = std::env::temp_dir().join(format!(
            "ctx-replay-bm-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let p = tmp.join("main.go");
        std::fs::write(&p, b"package main\n").unwrap();
        let input = BuildInput {
            id: "snap1".into(),
            created_at: "2026-05-14T00:00:00Z".into(),
            ctx_version: "dev".into(),
            goal: "review".into(),
            budget: 1000,
            used: 100,
            root: tmp.to_string_lossy().into(),
            format: "markdown".into(),
            included: vec![EntryInput {
                path: "main.go".into(),
                abs_path: p.to_string_lossy().into(),
                tokens: 50,
                relevance: "High".into(),
                score: 10,
                reason: "entry".into(),
            }],
            skipped: vec![SkippedInput {
                path: "dist/out.js".into(),
                reason: "ignored".into(),
            }],
            ..Default::default()
        };
        let m = build_manifest(input).unwrap();
        assert_eq!(m.schema_version, SCHEMA_VERSION);
        assert_eq!(m.id, "snap1");
        assert_eq!(m.entries.len(), 1);
        assert_eq!(m.entries[0].sha256.len(), 64);
        assert_eq!(m.entries[0].tokens, 50);
        assert_eq!(m.skipped.len(), 1);
    }
}
