// crates/ctx-replay/benches/sticky_handle.rs
//
// Rust-only intrinsic bench: measures the per-query overhead of the
// sticky-handle replay session against the stateless compute path,
// excluding cgo crossing cost. This is the floor of how fast the FFI
// session API can theoretically be served — any gap between this and
// the Go-side BenchmarkQueryDiff_Sessioned numbers is cgo +
// JSON-marshal overhead.
//
// Run:
//   cargo bench --bench sticky_handle --manifest-path crates/ctx-replay/Cargo.toml

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use ctx_replay::diff::{compute, DiffOptions};
use ctx_replay::session::ReplaySession;
use ctx_replay::store::open_store;
use ctx_replay::types::Manifest;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("replay-fixtures")
}

fn load_manifest_file(path: &std::path::Path) -> Option<Manifest> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn load_pair(name: &str) -> Option<(Manifest, Manifest)> {
    let dir = fixtures_dir().join(name);
    let base = load_manifest_file(&dir.join("base.json"))?;
    let cur = load_manifest_file(&dir.join("current.json"))?;
    Some((base, cur))
}

/// Build a temp store seeded with the given (id, manifest) pairs and
/// return the directory path.
fn seed_store(label: &str, manifests: &[(&str, &Manifest)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ctx-replay-bench-{}-{}-{}",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let store = open_store(dir.to_str().unwrap()).unwrap();
    for (id, m) in manifests {
        let mut copy = (*m).clone();
        copy.id = (*id).to_string();
        if copy.created_at.is_empty() {
            copy.created_at = "2026-01-01T00:00:00Z".into();
        }
        store.save(&copy).unwrap();
    }
    dir
}

// =====================================================================
// session.query_diff vs stateless compute
// =====================================================================

fn bench_diff(c: &mut Criterion) {
    let fixtures = ["single_snap", "multi_snap_drift", "scoring_change"];
    let mut group = c.benchmark_group("session_diff");
    for fx in &fixtures {
        let Some((mut base, cur)) = load_pair(fx) else {
            continue;
        };
        // Fix the base id so the seeded store can be queried.
        base.id = "base".into();
        if base.created_at.is_empty() {
            base.created_at = "2026-01-01T00:00:00Z".into();
        }
        let dir = seed_store(fx, &[("base", &base)]);

        // Pre-serialize the current manifest once — that's what the Go
        // caller will do too.
        let cur_json = serde_json::to_string(&cur).unwrap();
        let args = format!(
            r#"{{"base_id":"base","current_manifest":{},"strict":false}}"#,
            cur_json
        );

        let session = ReplaySession::open(dir.to_str().unwrap()).unwrap();
        group.bench_with_input(BenchmarkId::new("rust_session", fx), &args, |b, args| {
            b.iter(|| {
                let _ = session.query("diff", args).unwrap();
            });
        });

        // Stateless intrinsic (no FFI, just compute) — the lower bound.
        group.bench_with_input(
            BenchmarkId::new("rust_stateless", fx),
            &(base.clone(), cur.clone()),
            |b, (base, cur)| {
                b.iter(|| {
                    let _ = compute(base, cur, DiffOptions::default());
                });
            },
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
    group.finish();
}

// =====================================================================
// session.query_load (manifest cache hit) — what the web handler pays
// per repeated /api/replay/diff call against the same id.
// =====================================================================

fn bench_load(c: &mut Criterion) {
    let fixtures = ["single_snap", "multi_snap_drift", "scoring_change"];
    let mut group = c.benchmark_group("session_load");
    for fx in &fixtures {
        let Some((mut base, _cur)) = load_pair(fx) else {
            continue;
        };
        base.id = "base".into();
        if base.created_at.is_empty() {
            base.created_at = "2026-01-01T00:00:00Z".into();
        }
        let dir = seed_store(fx, &[("base", &base)]);
        let session = ReplaySession::open(dir.to_str().unwrap()).unwrap();
        // Warm the cache.
        let _ = session.query("load", r#"{"id":"base"}"#).unwrap();

        group.bench_with_input(BenchmarkId::new("rust_session", fx), &(), |b, _| {
            b.iter(|| {
                let _ = session.query("load", r#"{"id":"base"}"#).unwrap();
            });
        });

        let _ = std::fs::remove_dir_all(&dir);
    }
    group.finish();
}

// =====================================================================
// session.query_prune_candidates (cached list iteration)
// =====================================================================

fn bench_prune(c: &mut Criterion) {
    // Build a store with N stamped manifests and time the candidate
    // scan. The bench shape mirrors a web prune-preview call.
    let mut manifests: Vec<(String, Manifest)> = Vec::new();
    for i in 0..32 {
        let mut m = Manifest::default();
        m.schema_version = 1;
        m.id = format!("snap-{i:02}");
        m.created_at = format!("2026-04-{:02}T00:00:00Z", (i % 28) + 1);
        manifests.push((m.id.clone(), m));
    }
    let pairs: Vec<(&str, &Manifest)> = manifests.iter().map(|(id, m)| (id.as_str(), m)).collect();
    let dir = seed_store("prune", &pairs);
    let session = ReplaySession::open(dir.to_str().unwrap()).unwrap();
    // Warm the list cache.
    let _ = session.query("list", "{}").unwrap();
    let one_week_nanos: i64 = 7 * 24 * 3600 * 1_000_000_000;
    let args = format!(
        r#"{{"now":"2026-05-29T12:00:00Z","older_nanos":{}}}"#,
        one_week_nanos
    );

    let mut group = c.benchmark_group("session_prune");
    group.bench_with_input(
        BenchmarkId::new("rust_session", "32_snaps"),
        &args,
        |b, args| {
            b.iter(|| {
                let _ = session.query("prune_candidates", args).unwrap();
            });
        },
    );
    group.finish();

    let _ = std::fs::remove_dir_all(&dir);
}

criterion_group!(benches, bench_diff, bench_load, bench_prune);
criterion_main!(benches);
