use std::path::PathBuf;

use ctx_mix::{Draft, MixError, Store};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!(
            "ctx-mix-{tag}-{}-{}",
            std::process::id(),
            ctx_mix::generate_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn draft(name: &str, files: &[&str]) -> Draft {
    Draft {
        name: name.to_string(),
        files: files.iter().map(|f| f.to_string()).collect(),
        ..Draft::default()
    }
}

#[test]
fn creates_recipes_with_readable_unique_ids() {
    let tmp = TempDir::new("create");
    let store = Store::at(tmp.0.join("mixes"));
    let first = store
        .create(draft(
            "Auth review",
            &["src/a.rs", "./src/a.rs", "src/b.rs"],
        ))
        .unwrap();
    assert_eq!(first.id, "auth-review");
    assert_eq!(first.files, vec!["src/a.rs", "src/b.rs"]);
    let second = store.create(draft("auth review", &["x.rs"])).unwrap();
    assert_eq!(second.id, "auth-review-2");
    let random = store.create(draft("認証", &["x.rs"])).unwrap();
    assert_eq!(random.id.len(), 16);

    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 3);
    assert_eq!(store.load("auth-review").unwrap(), first);
}

#[test]
fn explicit_id_must_be_free() {
    let tmp = TempDir::new("explicit");
    let store = Store::at(&tmp.0);
    let mut d = draft("A", &["a.rs"]);
    d.id = "mine".to_string();
    store.create(d.clone()).unwrap();
    assert!(matches!(store.create(d), Err(MixError::Exists(_))));
}

#[test]
fn rejects_files_outside_the_repository() {
    let tmp = TempDir::new("escape");
    let store = Store::at(&tmp.0);
    assert!(store.create(draft("A", &["../secret"])).is_err());
    assert!(store.create(draft("A", &["/etc/passwd"])).is_err());
    assert!(store.create(draft("A", &[])).is_err());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn finds_by_id_or_unique_name() {
    let tmp = TempDir::new("find");
    let store = Store::at(&tmp.0);
    let mix = store.create(draft("Session Debug", &["a.rs"])).unwrap();
    assert_eq!(store.find("session-debug").unwrap().id, mix.id);
    assert_eq!(store.find("session debug").unwrap().id, mix.id);
    assert!(matches!(store.find("nope"), Err(MixError::NotFound(_))));
    let mut other = draft("Session Debug", &["b.rs"]);
    other.id = "other".to_string();
    store.create(other).unwrap();
    assert!(matches!(
        store.find("Session Debug"),
        Err(MixError::Ambiguous(..))
    ));
}

#[test]
fn updates_and_deletes() {
    let tmp = TempDir::new("update");
    let store = Store::at(&tmp.0);
    let mut mix = store.create(draft("A", &["a.rs"])).unwrap();
    mix.files.push("b.rs".to_string());
    mix.goal = "new goal".to_string();
    let updated = store.update(mix).unwrap();
    assert!(!updated.updated.is_empty());
    assert_eq!(store.load("a").unwrap().files, vec!["a.rs", "b.rs"]);
    store.delete("a").unwrap();
    assert!(matches!(store.load("a"), Err(MixError::NotFound(_))));
    assert!(matches!(store.delete("a"), Err(MixError::NotFound(_))));
    // Updating a recipe that does not exist does not create it.
    let ghost = ctx_mix::Mix {
        schema_version: 1,
        id: "ghost".to_string(),
        name: "g".to_string(),
        goal: String::new(),
        created: String::new(),
        updated: String::new(),
        files: vec!["a.rs".to_string()],
        budget: Default::default(),
    };
    assert!(store.update(ghost).is_err());
    assert!(!store.exists("ghost"));
}

#[cfg(unix)]
#[test]
fn planted_links_in_the_store_are_not_read() {
    let tmp = TempDir::new("links");
    let store_dir = tmp.0.join("mixes");
    std::fs::create_dir_all(&store_dir).unwrap();
    let outside = tmp.0.join("outside.json");
    std::fs::write(
        &outside,
        r#"{"schema_version":1,"id":"x","name":"x","created":"","files":["a"],"budget":{}}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(&outside, store_dir.join("x.mix.json")).unwrap();
    let store = Store::at(&store_dir);
    assert!(matches!(store.load("x"), Err(MixError::NotFound(_))));
    assert!(store.list().unwrap().is_empty());
    // The id stays taken, so a create never writes through the link.
    let created = store.create(draft("x", &["a.rs"])).unwrap();
    assert_eq!(created.id, "x-2");
    assert!(std::fs::read_to_string(&outside)
        .unwrap()
        .contains(r#""id":"x""#));
}

#[test]
fn reads_go_written_recipes() {
    let tmp = TempDir::new("go");
    std::fs::write(
        tmp.0.join("aabbccdd11223344.mix.json"),
        "{\n  \"schema_version\": 1,\n  \"id\": \"aabbccdd11223344\",\n  \"name\": \"Alpha Mix\",\n  \"goal\": \"g\",\n  \"created\": \"2026-01-01T10:00:00Z\",\n  \"files\": [\n    \"hello.txt\"\n  ],\n  \"budget\": {}\n}\n",
    )
    .unwrap();
    let mix = Store::at(&tmp.0).load("aabbccdd11223344").unwrap();
    assert_eq!(mix.files, vec!["hello.txt"]);
    let json = serde_json::to_string(&mix).unwrap();
    assert!(
        !json.contains("updated"),
        "an unedited recipe keeps the Go shape: {json}"
    );
}
