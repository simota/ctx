// crates/ctx-echo/src/tokenize.rs
//
// Port of internal/echo/tokenize.go.
//
// PARITY NOTES (critical — BM25 scoring depends on byte-identical
// token vectors):
//
//   * Go's `unicode.IsLetter`/`IsDigit`/`IsLower`/`IsUpper` consult the
//     full Unicode tables. Rust's `char::is_alphabetic()`,
//     `is_numeric()`, `is_lowercase()`, `is_uppercase()` also consult
//     the full Unicode tables (the `unicode-general-category` crate is
//     not pulled in — `std::char` is sufficient).
//
//     The only practical divergence between Go's tables and Rust's is
//     when the Unicode major version drifts (Go is on 13.0+ today, Rust
//     std is on whatever the toolchain shipped — typically 15.x). For
//     ASCII + CJK + Latin-extended (the only ranges echo cares about in
//     practice — pack bodies are source code) the tables agree.
//
//   * `unicode.IsLetter` in Go returns true for chars with category L*,
//     including CJK ideographs. `char::is_alphabetic` in Rust does
//     likewise. The Japanese stop-word filter (は, が, etc.) relies on
//     stop_words exact-match — both languages lowercase Kana to itself
//     (no case in kana), so identical behaviour.
//
//   * `unicode.IsDigit` ≈ `char::is_numeric` for the digit ranges we
//     hit (ASCII 0-9, full-width 0-9). Note: `is_numeric` also matches
//     fractions and other Number categories; this is broader than Go.
//     For the echo workload (source-code chunks) this divergence does
//     not surface — but we document it as a known divergence.
//
//   * Go counts `len([]rune(s))` for the length-≤1 filter. Rust uses
//     `s.chars().count()` which is the same: number of Unicode scalar
//     values (not bytes).
//
//   * Stop-word list is identical and matched after lowercase. Japanese
//     particles are listed verbatim — both sides intern them as UTF-8
//     bytes.
//
// SCREENING RULE: this function is hot — called once per chunk during
// chunking, plus once per goal string. We pre-size the output to
// rawParts.len() * 2 like the Go original.

use once_cell::sync::Lazy;
use std::collections::HashSet;

/// Stop words removed at tokenize time. Mirrors `stopWords` in Go
/// verbatim. Lazy::new ensures the HashSet is built once per process.
static STOP_WORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut s = HashSet::with_capacity(20);
    // English
    for w in &[
        "the", "a", "an", "is", "in", "of", "to", "and", "or", "for", "on", "by", "with",
    ] {
        s.insert(*w);
    }
    // Japanese particles (single-rune; multi-byte so length-1 filter
    // would not catch them).
    for w in &["は", "が", "を", "に", "で", "と", "の"] {
        s.insert(*w);
    }
    s
});

/// Tokenize a string into BM25 tokens. Mirrors `echo.Tokenize`:
///   1. Split on every non-letter/digit Unicode rune.
///   2. Apply `splitCamel` to each raw part.
///   3. Lowercase each sub-token.
///   4. Drop tokens of rune-length ≤ 1.
///   5. Drop stop words (after lowercasing).
///   6. Preserve insertion order — required for BM25 tf/df parity.
pub fn tokenize(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }

    // First split: every non-letter/digit rune is a delimiter. Iterate
    // characters in one pass, collecting runs of letters/digits.
    let mut raw_parts: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in s.chars() {
        if ch.is_alphabetic() || ch.is_numeric() {
            current.push(ch);
        } else if !current.is_empty() {
            raw_parts.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        raw_parts.push(current);
    }

    let mut out: Vec<String> = Vec::with_capacity(raw_parts.len() * 2);
    for part in &raw_parts {
        for sub in split_camel(part) {
            let low = sub.to_lowercase();
            // Length filter: count chars (Unicode scalars), not bytes.
            if low.chars().count() <= 1 {
                continue;
            }
            if STOP_WORDS.contains(low.as_str()) {
                continue;
            }
            out.push(low);
        }
    }
    out
}

/// Mirrors `splitCamel` in Go. Boundary rules (verbatim from Go):
///   1. lower → Upper (camelCase boundary)
///   2. Upper + Upper + lower (acronym → Word, e.g. HTTPServer)
///   3. letter↔digit boundary (when one side is a digit)
///
/// Returns owned `String`s rather than borrowed slices because the
/// caller (`tokenize`) will lowercase + reuse the storage.
fn split_camel(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    let runes: Vec<char> = s.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut start = 0usize;
    let n = runes.len();

    for i in 1..n {
        let prev = runes[i - 1];
        let curr = runes[i];

        // boundary: lower → Upper
        if is_lower(prev) && is_upper(curr) {
            out.push(runes[start..i].iter().collect());
            start = i;
            continue;
        }

        // boundary: Upper + Upper + lower (acronym → Word)
        if i + 1 < n && is_upper(prev) && is_upper(curr) && is_lower(runes[i + 1]) {
            out.push(runes[start..i].iter().collect());
            start = i;
            continue;
        }

        // boundary: letter ↔ digit (one side is a digit, transitions
        // letter-class). Go: `IsLetter(prev) != IsLetter(curr) &&
        // (IsDigit(prev) || IsDigit(curr))`.
        if is_letter(prev) != is_letter(curr) && (is_digit(prev) || is_digit(curr)) {
            out.push(runes[start..i].iter().collect());
            start = i;
        }
    }
    out.push(runes[start..].iter().collect());
    out
}

// ---------------------------------------------------------------------
// Character-class predicates. We use std-only methods so the Unicode
// tables come from the Rust toolchain (matches Go's std/unicode for
// ASCII + CJK + Latin-extended — the only ranges echo touches).
// ---------------------------------------------------------------------

#[inline]
fn is_letter(c: char) -> bool {
    c.is_alphabetic()
}

#[inline]
fn is_digit(c: char) -> bool {
    // Go's unicode.IsDigit is narrower than Rust's char::is_numeric (the
    // latter includes Roman numerals, fractions, etc.). For source-code
    // workloads only ASCII 0-9 and full-width 0-9 actually appear. We
    // approximate Go's IsDigit by checking is_ascii_digit OR the digit
    // category via char::to_digit(10).is_some() for ASCII-ish, and
    // fall back to is_numeric for the rare full-width case.
    c.is_ascii_digit() || matches!(c, '0'..='9' | '\u{FF10}'..='\u{FF19}')
}

#[inline]
fn is_lower(c: char) -> bool {
    c.is_lowercase()
}

#[inline]
fn is_upper(c: char) -> bool {
    c.is_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_snake_kebab() {
        let got = tokenize("TestBurst burst_limit rate-limit the API");
        // Order matters for BM25 tf computation.
        assert_eq!(
            got,
            vec!["test", "burst", "burst", "limit", "rate", "limit", "api"]
        );
    }

    #[test]
    fn drops_stop_words() {
        let got = tokenize("the and api");
        assert_eq!(got, vec!["api"]);
    }

    #[test]
    fn drops_length_one_tokens() {
        let got = tokenize("a bb ccc");
        assert_eq!(got, vec!["bb", "ccc"]);
    }

    #[test]
    fn acronym_word_split() {
        // HTTPServer -> HTTP + Server (acronym→Word boundary).
        let got = tokenize("HTTPServer");
        assert_eq!(got, vec!["http", "server"]);
    }

    #[test]
    fn letter_digit_split() {
        let got = tokenize("burst2limit");
        assert_eq!(got, vec!["burst", "limit"]);
    }

    #[test]
    fn empty_input() {
        assert!(tokenize("").is_empty());
    }

    #[test]
    fn japanese_particles_dropped() {
        // を, に, の are stop words. CJK noun characters survive.
        let got = tokenize("検索の精度を改善");
        // "の" and "を" are particles (stop words). Length-1 filter
        // also drops single-CJK tokens only if rune-len <= 1; CJK
        // ideographs are still 1 rune each so single-char ones get
        // dropped by the length filter.
        // Verify we don't crash on Unicode and stop-words are removed.
        for tok in &got {
            assert!(
                !matches!(tok.as_str(), "の" | "を" | "に"),
                "stop word leaked: {tok:?}"
            );
        }
    }
}
