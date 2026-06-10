// crates/ctx-braid/src/allocate.rs
//
// Port of internal/braid/allocate.go — share-weighted budget
// distribution.
//
// Pure aside from the optional warning text returned alongside the
// allocations. Mirrors Go's `Allocate` 1:1.

use crate::types::{Allocation, Config};

/// Result of an Allocate call. `warning` is non-empty when the share
/// total exceeded 1.0+epsilon and shares were normalised. Mirrors Go's
/// warning sink behaviour but returns the string rather than writing it
/// to an `io.Writer` (FFI-friendly).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AllocateOutput {
    pub allocations: Vec<Allocation>,
    pub warning: String,
}

const EPS: f64 = 1e-9;

/// Allocate computes the per-strand budget. When the share total is
/// greater than 1.0+epsilon, shares are normalised proportionally and a
/// warning string is returned. A total below 1.0 is allowed and not
/// normalised — the user may intentionally reserve headroom.
///
/// Mirrors Go's `Allocate(cfg Config, globalBudget int, warn io.Writer)`
/// — but writes the warning into `AllocateOutput.warning` so the FFI
/// surface can return it explicitly.
pub fn allocate(cfg: &Config, global_budget: i64) -> AllocateOutput {
    let mut total: f64 = 0.0;
    for s in &cfg.strands {
        total += s.share;
    }

    let mut shares: Vec<f64> = Vec::with_capacity(cfg.strands.len());
    let mut warning = String::new();
    if total > 1.0 + EPS {
        // Go format: "warning: braid strand shares total %.3f > 1.0; normalising to 1.0\n"
        warning = format!(
            "warning: braid strand shares total {:.3} > 1.0; normalising to 1.0\n",
            total
        );
        for s in &cfg.strands {
            shares.push(s.share / total);
        }
    } else {
        for s in &cfg.strands {
            shares.push(s.share);
        }
    }

    let mut out: Vec<Allocation> = Vec::with_capacity(cfg.strands.len());
    for (i, s) in cfg.strands.iter().enumerate() {
        let share = shares[i];
        let budget_f = (global_budget as f64) * share;
        // math.Round rounds to nearest, ties away from zero — matches
        // Rust's f64::round.
        let budget = budget_f.round() as i64;
        out.push(Allocation {
            name: s.name.clone(),
            share,
            budget,
            policy: s.policy.unwrap_or_merge(),
        });
    }

    AllocateOutput {
        allocations: out,
        warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PolicyKindOrEmpty, Strand};

    fn strand(name: &str, share: f64) -> Strand {
        Strand {
            name: name.into(),
            source: "where 'x'".into(),
            share,
            policy: PolicyKindOrEmpty::default(),
        }
    }

    #[test]
    fn under_one_preserved_no_warning() {
        let cfg = Config {
            schema_version: 1,
            strands: vec![strand("a", 0.3), strand("b", 0.4)],
        };
        let out = allocate(&cfg, 1000);
        assert!(out.warning.is_empty());
        assert_eq!(out.allocations[0].budget, 300);
        assert_eq!(out.allocations[1].budget, 400);
    }

    #[test]
    fn over_one_normalises_with_warning() {
        let cfg = Config {
            schema_version: 1,
            strands: vec![
                strand("a", 0.7),
                strand("b", 0.6),
            ],
        };
        let out = allocate(&cfg, 10_000);
        assert!(out.warning.contains("normalising to 1.0"));
        let total: f64 = out.allocations.iter().map(|a| a.share).sum();
        assert!((total - 1.0).abs() < 1e-6);
        let sum_budget: i64 = out.allocations.iter().map(|a| a.budget).sum();
        assert!((9990..=10010).contains(&sum_budget));
    }
}
