// crates/ctx-tokens/src/split.rs
//
// Hand-written equivalent of the cl100k_base pre-tokenizer regex, as
// tiktoken-rs compiles it with fancy-regex:
//
//   '(?i:[sdmt]|ll|ve|re)            A1  contractions
//   |[^\r\n\p{L}\p{N}]?+\p{L}++      A2  optional lead char + letters
//   |\p{N}{1,3}+                     A3  1..=3 numbers
//   | ?[^\s\p{L}\p{N}]++[\r\n]*+     A4  punctuation run (+ newlines)
//   |\s++$                           A5  trailing whitespace
//   |\s*[\r\n]                       A6  whitespace up to the last newline
//   |\s+(?!\S)                       A7  whitespace but the last char
//   |\s                              A8  one whitespace char
//
// Profiling showed the fancy-regex backtracking VM is ~90% of the
// encoder's time; this scanner yields the same pieces without it. It
// follows the backtracking (leftmost-first) semantics alternative by
// alternative — see the comments in `piece_end`. Every char is a letter,
// a number, whitespace or "other", and some alternative accepts each of
// those, so pieces tile the input exactly like `find_iter` does.
//
// The character classes are taken from regex-syntax (the same Unicode
// tables fancy-regex/regex use), not from `char::is_*`, so the Unicode
// version cannot drift between the two. Exactness is pinned by the
// `split_matches_fancy_regex` tests against the real pattern.

use regex_syntax::hir::{Class, HirKind};

const LETTER: u8 = 1;
const NUMBER: u8 = 2;
const SPACE: u8 = 4;
/// `'` + one of these (case-insensitively) is a whole contraction.
const FOLD_SDMT: u8 = 8;
const FOLD_L: u8 = 16;
const FOLD_V: u8 = 32;
const FOLD_R: u8 = 64;
const FOLD_E: u8 = 128;

pub(crate) struct Classes {
    ascii: [u8; 128],
    /// Sorted, non-overlapping inclusive ranges per flag (non-ASCII lookup).
    ranges: Vec<(u8, Vec<(char, char)>)>,
}

fn class_ranges(pattern: &str) -> Vec<(char, char)> {
    let hir = regex_syntax::Parser::new()
        .parse(pattern)
        .expect("static class pattern parses");
    match hir.kind() {
        HirKind::Class(Class::Unicode(class)) => class
            .ranges()
            .iter()
            .map(|range| (range.start(), range.end()))
            .collect(),
        other => panic!("{pattern} is not a Unicode class: {other:?}"),
    }
}

impl Classes {
    pub(crate) fn new() -> Self {
        let ranges: Vec<(u8, Vec<(char, char)>)> = [
            (LETTER, r"\p{L}"),
            (NUMBER, r"\p{N}"),
            (SPACE, r"\s"),
            (FOLD_SDMT, r"(?i:[sdmt])"),
            (FOLD_L, r"(?i:l)"),
            (FOLD_V, r"(?i:v)"),
            (FOLD_R, r"(?i:r)"),
            (FOLD_E, r"(?i:e)"),
        ]
        .into_iter()
        .map(|(flag, pattern)| (flag, class_ranges(pattern)))
        .collect();
        let mut ascii = [0u8; 128];
        for (byte, slot) in ascii.iter_mut().enumerate() {
            *slot = lookup(&ranges, char::from(byte as u8));
        }
        Self { ascii, ranges }
    }

    #[inline]
    fn flags(&self, ch: char) -> u8 {
        if ch.is_ascii() {
            self.ascii[ch as usize]
        } else {
            lookup(&self.ranges, ch)
        }
    }
}

fn lookup(ranges: &[(u8, Vec<(char, char)>)], ch: char) -> u8 {
    let mut out = 0;
    for (flag, list) in ranges {
        let idx = list.partition_point(|&(_, end)| end < ch);
        if list.get(idx).is_some_and(|&(start, _)| start <= ch) {
            out |= flag;
        }
    }
    out
}

/// Iterator over the cl100k_base pre-tokenizer pieces of a string.
pub(crate) struct Pieces<'a> {
    text: &'a str,
    pos: usize,
    classes: &'a Classes,
}

impl<'a> Pieces<'a> {
    pub(crate) fn new(text: &'a str, classes: &'a Classes) -> Self {
        Self {
            text,
            pos: 0,
            classes,
        }
    }

    // `inline(always)`: left to `#[inline]`, the compiler kept this an
    // out-of-line call per char, ~18% of counting's instructions.
    #[inline(always)]
    fn char_at(&self, at: usize) -> Option<(char, u8)> {
        // ASCII fast path: no UTF-8 decoding for the common case.
        let &byte = self.text.as_bytes().get(at)?;
        if byte.is_ascii() {
            return Some((char::from(byte), self.classes.ascii[usize::from(byte)]));
        }
        let ch = self.text[at..].chars().next()?;
        Some((ch, self.classes.flags(ch)))
    }

    /// End of the maximal run starting at `at` of chars satisfying `keep`.
    #[inline(always)]
    fn run_end(&self, mut at: usize, keep: impl Fn(char, u8) -> bool) -> usize {
        while let Some((ch, flags)) = self.char_at(at) {
            if !keep(ch, flags) {
                break;
            }
            at += ch.len_utf8();
        }
        at
    }

    /// End (exclusive) of the piece that starts at `start`.
    fn piece_end(&self, start: usize) -> usize {
        let (c0, f0) = self.char_at(start).expect("start < len");
        let i1 = start + c0.len_utf8();
        let next = self.char_at(i1);
        let is_other = |flags: u8| flags & (LETTER | NUMBER | SPACE) == 0;

        // A1: '(?i:[sdmt]|ll|ve|re)
        if c0 == '\'' {
            if let Some((c1, f1)) = next {
                let i2 = i1 + c1.len_utf8();
                if f1 & FOLD_SDMT != 0 {
                    return i2;
                }
                if let Some((c2, f2)) = self.char_at(i2) {
                    let pair = (f1 & FOLD_L != 0 && f2 & FOLD_L != 0)
                        || (f1 & FOLD_V != 0 && f2 & FOLD_E != 0)
                        || (f1 & FOLD_R != 0 && f2 & FOLD_E != 0);
                    if pair {
                        return i2 + c2.len_utf8();
                    }
                }
            }
        }

        // A2: [^\r\n\p{L}\p{N}]?+\p{L}++ — the optional char is possessive:
        // when c0 qualifies it is taken and letters must follow at i1 (no
        // retry without it); otherwise letters must start at c0.
        let letters = |_: char, flags: u8| flags & LETTER != 0;
        if f0 & LETTER != 0 {
            return self.run_end(i1, letters);
        }
        if f0 & NUMBER == 0 && c0 != '\r' && c0 != '\n' {
            if let Some((_, f1)) = next {
                if f1 & LETTER != 0 {
                    return self.run_end(i1, letters);
                }
            }
        }

        // A3: \p{N}{1,3}+
        if f0 & NUMBER != 0 {
            let mut end = i1;
            for _ in 0..2 {
                match self.char_at(end) {
                    Some((ch, flags)) if flags & NUMBER != 0 => end += ch.len_utf8(),
                    _ => break,
                }
            }
            return end;
        }

        // A4: ` ?[^\s\p{L}\p{N}]++[\r\n]*+` — the space is plain greedy, so
        // it is taken only when an "other" char follows it; a space can
        // never start the run itself (it is \s).
        let run_from = if c0 == ' ' && next.is_some_and(|(_, f1)| is_other(f1)) {
            Some(i1)
        } else if is_other(f0) {
            Some(start)
        } else {
            None
        };
        if let Some(from) = run_from {
            let end = self.run_end(from, |_, flags| is_other(flags));
            return self.run_end(end, |ch, _| ch == '\r' || ch == '\n');
        }

        // Only whitespace remains (c0 is \s).
        let ws_end = self.run_end(start, |_, flags| flags & SPACE != 0);
        // A5: \s++$ — the whole run must reach the end of the text.
        if ws_end == self.text.len() {
            return ws_end;
        }
        // A6: \s*[\r\n] — greedy \s* backtracks to the LAST \r/\n of the run.
        if let Some(last_nl) = self.text[start..ws_end].rfind(['\r', '\n']) {
            return start + last_nl + 1;
        }
        // A7: \s+(?!\S) — the run is followed by \S, so drop its last char
        // (needs at least one char left).
        let last_start = self.text[start..ws_end]
            .char_indices()
            .next_back()
            .map_or(start, |(idx, _)| start + idx);
        if last_start > start {
            return last_start;
        }
        // A8: \s
        i1
    }
}

impl<'a> Iterator for Pieces<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.pos >= self.text.len() {
            return None;
        }
        let start = self.pos;
        self.pos = self.piece_end(start);
        Some(&self.text[start..self.pos])
    }
}
