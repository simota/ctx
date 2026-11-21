// crates/ctx-relations/src/types.rs
//
// Port of the subset of internal/relations and internal/model types the
// crate exposes across the FFI:
//
//   - relations.Index   (internal/relations/relations.go)
//   - Edges helper      (internal/relations/relations.go)
//
// JSON shape mirrors the Go-side golden exporter (cmd/relations-golden-export):
// snake_case keys, sorted map entries with stable ordering, no nulls.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `Index` mirrors `relations.Index`. The Go type uses Go-style field
/// names with no JSON tags; the golden exporter normalises them to
/// snake_case so this struct's serialized form matches byte-exact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    /// ModulePath from go.mod ("" when no go.mod at the root).
    #[serde(rename = "module_path")]
    pub module_path: String,

    /// Outgoing imports: file → sorted/deduped list of files it imports.
    /// Ordering is deterministic (BTreeMap keys) and matches the Go
    /// exporter which sorts keys before emitting.
    #[serde(rename = "imports")]
    pub imports: BTreeMap<String, Vec<String>>,

    /// Reverse index: file → sorted/deduped list of files that import it.
    #[serde(rename = "importers")]
    pub importers: BTreeMap<String, Vec<String>>,
}

impl Index {
    /// Mirrors `(*Index).Edges(file)`. Returns sorted/deduped
    /// (imports, importers) for `file`. Missing entries return empty
    /// (non-nil) vectors — matching the Go contract.
    pub fn edges(&self, file: &str) -> Edges {
        let mut imports = self.imports.get(file).cloned().unwrap_or_default();
        let mut importers = self.importers.get(file).cloned().unwrap_or_default();
        imports.sort();
        importers.sort();
        Edges { imports, importers }
    }
}

/// Sorted, deduped (imports, importers) pair returned by `Index::edges`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edges {
    pub imports: Vec<String>,
    pub importers: Vec<String>,
}

/// Which way `Index::reach` follows the import graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Files the seeds import (their dependencies).
    Imports,
    /// Files that import the seeds (their dependents).
    Importers,
}

/// A file reached from the seeds of `Index::reach`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reached {
    pub path: String,
    /// Import hops from the nearest seed (1 = direct).
    pub depth: usize,
    /// The file one hop closer to the seeds this one was reached through
    /// (the first in path order when there are several).
    pub via: String,
    /// How many files one hop closer link to this one.
    pub links: usize,
}

impl Index {
    /// Files within `max_depth` import hops of `seeds`, nearest first; at
    /// equal depth the most linked first, then by path. Seeds themselves are
    /// never returned, and each file appears once, at its shortest distance.
    pub fn reach(&self, seeds: &[String], direction: Direction, max_depth: usize) -> Vec<Reached> {
        let graph = match direction {
            Direction::Imports => &self.imports,
            Direction::Importers => &self.importers,
        };
        let mut seen: std::collections::BTreeSet<&str> = seeds.iter().map(String::as_str).collect();
        let mut frontier: Vec<&str> = seen.iter().copied().collect();
        let mut out = Vec::new();
        for depth in 1..=max_depth {
            // BTreeMap: `via` is the first linking file in path order.
            let mut level: BTreeMap<&str, (&str, usize)> = BTreeMap::new();
            frontier.sort_unstable();
            for from in &frontier {
                for to in graph.get(*from).into_iter().flatten() {
                    if seen.contains(to.as_str()) {
                        continue;
                    }
                    level.entry(to.as_str()).or_insert((from, 0)).1 += 1;
                }
            }
            if level.is_empty() {
                break;
            }
            let mut found: Vec<Reached> = level
                .iter()
                .map(|(path, (via, links))| Reached {
                    path: path.to_string(),
                    depth,
                    via: via.to_string(),
                    links: *links,
                })
                .collect();
            found.sort_by(|a, b| b.links.cmp(&a.links).then_with(|| a.path.cmp(&b.path)));
            frontier = level.keys().copied().collect();
            seen.extend(frontier.iter().copied());
            out.extend(found);
        }
        out
    }
}

#[cfg(test)]
mod reach_tests {
    use super::*;

    fn index(edges: &[(&str, &[&str])]) -> Index {
        let mut index = Index::default();
        for (from, tos) in edges {
            for to in *tos {
                index
                    .imports
                    .entry(from.to_string())
                    .or_default()
                    .push(to.to_string());
                index
                    .importers
                    .entry(to.to_string())
                    .or_default()
                    .push(from.to_string());
            }
        }
        index
    }

    fn paths(found: &[Reached]) -> Vec<(&str, usize)> {
        found.iter().map(|r| (r.path.as_str(), r.depth)).collect()
    }

    #[test]
    fn reach_walks_imports_breadth_first_and_stops_at_depth() {
        let idx = index(&[
            ("a", &["b", "c"]),
            ("b", &["d"]),
            ("d", &["e"]),
            ("c", &["a"]),
        ]);
        let seeds = ["a".to_string()];
        assert_eq!(
            paths(&idx.reach(&seeds, Direction::Imports, 1)),
            [("b", 1), ("c", 1)]
        );
        let two = idx.reach(&seeds, Direction::Imports, 2);
        assert_eq!(paths(&two), [("b", 1), ("c", 1), ("d", 2)]);
        assert_eq!(two[2].via, "b");
        // A cycle back to the seed and exhausted graphs both terminate.
        assert_eq!(paths(&idx.reach(&seeds, Direction::Imports, 9)).len(), 4);
    }

    #[test]
    fn reach_ranks_by_links_and_follows_importers() {
        let idx = index(&[("a", &["x", "y"]), ("b", &["y"]), ("t", &["a"])]);
        let seeds = ["a".to_string(), "b".to_string()];
        let found = idx.reach(&seeds, Direction::Imports, 1);
        assert_eq!(paths(&found), [("y", 1), ("x", 1)]);
        assert_eq!((found[0].links, found[0].via.as_str()), (2, "a"));
        assert_eq!(
            paths(&idx.reach(&["y".to_string()], Direction::Importers, 2)),
            [("a", 1), ("b", 1), ("t", 2)]
        );
        assert!(idx.reach(&seeds, Direction::Imports, 0).is_empty());
    }
}
