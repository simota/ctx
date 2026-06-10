// crates/ctx-scan/src/entropy.rs
//
// Port of internal/scan/entropy.go.
//
// Shannon entropy over the rune (Unicode scalar) frequency distribution.
// Go's `range s` iterates runes; we mirror that with `s.chars()`. The
// denominator is the rune count (not the byte count) — important for
// strings carrying non-ASCII content because byte length would give a
// different result.

use std::collections::HashMap;

/// `shannon_entropy` — mirrors `shannonEntropy(s string) float64`.
///
/// Empty input returns 0.0 (matching the Go early-return).
pub fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut counts: HashMap<char, u64> = HashMap::new();
    let mut total: u64 = 0;
    for r in s.chars() {
        *counts.entry(r).or_insert(0) += 1;
        total += 1;
    }
    let length = total as f64;
    let mut entropy = 0.0_f64;
    for &count in counts.values() {
        let p = count as f64 / length;
        entropy -= p * p.log2();
    }
    entropy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_is_zero() {
        assert_eq!(shannon_entropy(""), 0.0);
    }

    #[test]
    fn single_repeated_char_is_zero() {
        assert_eq!(shannon_entropy("aaaa"), 0.0);
    }

    #[test]
    fn uniform_two_chars_is_one() {
        // For "ab" each char has p=0.5, entropy = -2 * (0.5 * log2(0.5)) = 1.0
        let e = shannon_entropy("ab");
        assert!((e - 1.0).abs() < 1e-12, "got {e}");
    }

    #[test]
    fn high_entropy_random_like_string() {
        // Long pseudo-random alphanumeric — must clear the 4.0 threshold
        // used by the Go code path for the high_entropy warning.
        let s = "abcdefghijklmnopqrstuvwxyzABCDEF1234567890";
        let e = shannon_entropy(s);
        assert!(e >= 4.0, "expected >= 4.0, got {e}");
    }
}
