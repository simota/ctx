//! Files added around a pack selection: what the selected files import
//! (`--deps`), what imports them (`--dependents`), the tests that exercise
//! them (`--tests`) and what historically changes with them (`--companions`).

use std::path::Path;

use super::*;

/// Why a file was added around the selection. The order is the order the
/// groups are packed in: definitions first, then callers, then tests, then
/// companions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdditionKind {
    Dependency = 0,
    Dependent = 1,
    Test = 2,
    Companion = 3,
}

#[derive(Debug, Clone)]
pub(crate) struct PackAddition {
    pub(crate) kind: AdditionKind,
    pub(crate) path: String,
    pub(crate) reason: String,
}

/// Whether any of `--deps`, `--dependents`, `--tests` or `--companions` is set.
pub(crate) fn wants_additions(args: &PackArgs) -> bool {
    args.deps > 0 || args.dependents > 0 || args.tests > 0 || args.companions > 0
}

/// Hands out additions in order while enforcing each flag's limit and never
/// offering the same path twice.
pub(crate) struct AdditionQuota {
    left: [usize; 4],
    taken: std::collections::BTreeSet<String>,
}

impl AdditionQuota {
    pub(crate) fn new(args: &PackArgs) -> Self {
        Self {
            left: [args.deps, args.dependents, args.tests, args.companions],
            taken: Default::default(),
        }
    }

    /// Whether `addition` may still be packed. Call `take` once it is.
    pub(crate) fn allows(&self, addition: &PackAddition) -> bool {
        self.left[addition.kind as usize] > 0 && !self.taken.contains(&addition.path)
    }

    pub(crate) fn take(&mut self, addition: &PackAddition) {
        self.left[addition.kind as usize] -= 1;
        self.taken.insert(addition.path.clone());
    }
}

fn import_reason(found: &ctx_relations::Reached, dependency: bool) -> String {
    let others = match found.links.saturating_sub(1) {
        0 => String::new(),
        n => format!(" and {n} other selected file(s)"),
    };
    if dependency {
        format!("dependency: imported by {}{others}", found.via)
    } else {
        format!("dependent: imports {}{others}", found.via)
    }
}

/// Every candidate addition for `seeds` (paths relative to `root`), most
/// useful first within each kind. Callers keep those `AdditionQuota` allows
/// and that they can actually pack.
pub(crate) fn pack_additions(root: &Path, seeds: &[String], args: &PackArgs) -> Vec<PackAddition> {
    let mut out = Vec::new();
    if seeds.is_empty() {
        return out;
    }
    let index = if args.deps > 0 || args.dependents > 0 || args.tests > 0 {
        match ctx_relations::build(&root.to_string_lossy()) {
            Ok(index) => Some(index),
            Err(err) => {
                if !args.no_warnings {
                    eprintln!("warning: --deps/--dependents/--tests: cannot read imports: {err}");
                }
                None
            }
        }
    } else {
        None
    };
    if let Some(index) = index.as_ref() {
        for (wanted, direction, kind, flag) in [
            (
                args.deps,
                ctx_relations::Direction::Imports,
                AdditionKind::Dependency,
                "--deps",
            ),
            (
                args.dependents,
                ctx_relations::Direction::Importers,
                AdditionKind::Dependent,
                "--dependents",
            ),
        ] {
            if wanted == 0 {
                continue;
            }
            let found = index.reach(seeds, direction, 1);
            if found.is_empty() && !args.no_warnings {
                let what = if kind == AdditionKind::Dependency {
                    "the selected files import no file here"
                } else {
                    "no file here imports the selected files"
                };
                eprintln!("warning: {flag}: {what} (imports are resolved for Rust, Go, JS/TS, Python, Java/Kotlin, PHP and Swift)");
            }
            out.extend(found.iter().map(|found| PackAddition {
                kind,
                path: found.path.clone(),
                reason: import_reason(found, kind == AdditionKind::Dependency),
            }));
        }
    }
    if args.tests > 0 {
        let files: Vec<String> = ctx_relations::walk::walk(&root.to_string_lossy())
            .map(|walked| {
                walked
                    .into_iter()
                    .filter(|file| !file.is_dir && file.path != ".")
                    .map(|file| file.path)
                    .collect()
            })
            .unwrap_or_default();
        let found = ctx_relations::find_tests(&files, index.as_ref(), seeds);
        if found.is_empty() && !args.no_warnings {
            eprintln!(
                "warning: --tests: no test file is named after or imports the selected files"
            );
        }
        out.extend(found.into_iter().map(|link| PackAddition {
            kind: AdditionKind::Test,
            reason: link.reason(),
            path: link.path,
        }));
    }
    if args.companions > 0 {
        out.extend(
            pack_companions(root, seeds, args.no_warnings)
                .into_iter()
                .map(|(path, reason)| PackAddition {
                    kind: AdditionKind::Companion,
                    path,
                    reason,
                }),
        );
    }
    out
}
