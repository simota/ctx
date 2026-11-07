// Exactness tests: the regex-free counter must agree with tiktoken-rs
// (`cl100k_base().encode_ordinary`) and its split pattern piece-for-piece.

use rustc_hash::FxHashMap as HashMap;
use std::path::Path;

use super::*;

/// The pattern tiktoken-rs 0.11 compiles for cl100k_base (copied verbatim).
const CL100K_PATTERN: &str = "'(?i:[sdmt]|ll|ve|re)|[^\\r\\n\\p{L}\\p{N}]?+\\p{L}++|\\p{N}{1,3}+| ?[^\\s\\p{L}\\p{N}]++[\\r\\n]*+|\\s++$|\\s*[\\r\\n]|\\s+(?!\\S)|\\s";

fn oracle() -> &'static tiktoken_rs::CoreBPE {
    static ORACLE: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();
    ORACLE.get_or_init(|| tiktoken_rs::cl100k_base().unwrap())
}

fn oracle_regex() -> &'static fancy_regex::Regex {
    static REGEX: OnceLock<fancy_regex::Regex> = OnceLock::new();
    REGEX.get_or_init(|| fancy_regex::Regex::new(CL100K_PATTERN).unwrap())
}

fn regex_pieces(text: &str) -> Vec<&str> {
    oracle_regex()
        .find_iter(text)
        .map(|m| m.unwrap().as_str())
        .collect()
}

fn fast_pieces(text: &str) -> Vec<&str> {
    split::Pieces::new(text, classes()).collect()
}

fn assert_same(text: &str) {
    assert_eq!(fast_pieces(text), regex_pieces(text), "pieces of {text:?}");
    assert_eq!(
        count_str(text),
        oracle().encode_ordinary(text).len() as i64,
        "count of {text:?}"
    );
}

/// Chars covering every class the pattern distinguishes, the contraction
/// letters (incl. U+017F, which case-folds to `s`), newline-like spaces and
/// multi-byte encodings.
const ALPHABET: &[char] = &[
    'a', 'Z', 's', 'S', 'ſ', 'd', 'M', 't', 'l', 'L', 'v', 'V', 'e', 'E', 'r', 'R', 'é', '日', 'K',
    '\'', '"', '1', '٣', 'Ⅷ', '½', ' ', '\t', '\r', '\n', '\u{a0}', '\u{85}', '\u{2028}',
    '\u{3000}', '.', '(', '_', '-', '\u{301}', '😀', '\u{0}',
];

/// tiktoken-rs's rank map, rebuilt from its decoder.
fn oracle_ranks() -> &'static HashMap<Vec<u8>, Rank> {
    static RANKS: OnceLock<HashMap<Vec<u8>, Rank>> = OnceLock::new();
    RANKS.get_or_init(|| {
        (0..100_256)
            .map(|rank| (oracle().decode_bytes(&[rank]).unwrap(), rank))
            .collect()
    })
}

#[test]
fn vocabulary_matches_tiktoken() {
    assert_eq!(vocab::len(), 100_256);
    assert_eq!(oracle_ranks().len(), 100_256);
    for rank in 0..100_256 {
        let bytes = oracle().decode_bytes(&[rank]).unwrap();
        assert_eq!(vocab::token(rank), bytes.as_slice(), "rank {rank}");
        assert_eq!(vocab::rank(&bytes), Some(rank), "rank {rank}");
    }
}

#[test]
fn vocabulary_rejects_exactly_the_non_tokens() {
    // Byte strings next to every token (one byte longer or shorter), most
    // of which are not tokens: a lookup must find exactly what tiktoken's
    // map holds.
    let mut checked_misses = 0;
    for rank in 0..100_256 {
        let bytes = vocab::token(rank);
        let mut probes = vec![bytes[..bytes.len() - 1].to_vec(), bytes[1..].to_vec()];
        for extra in [b' ', b'e', 0x80, 0xff] {
            let mut longer = bytes.to_vec();
            longer.push(extra);
            probes.push(longer);
        }
        for probe in probes {
            let expected = oracle_ranks().get(&probe).copied();
            checked_misses += usize::from(expected.is_none());
            assert_eq!(vocab::rank(&probe), expected, "{probe:?}");
        }
    }
    assert!(
        checked_misses > 100_000,
        "only {checked_misses} misses checked"
    );
    assert_eq!(vocab::rank(b""), None);
}

#[test]
fn byte_pair_count_matches_tiktoken_merge() {
    let mut parts = Vec::new();
    let mut check = |piece: &[u8]| {
        assert_eq!(
            byte_pair_count(piece, &mut parts),
            tiktoken_rs::byte_pair_split(piece, oracle_ranks()).len(),
            "{piece:?}"
        );
    };
    for rank in (0..100_256).step_by(7) {
        // Tokens glued to their neighbours: long, mostly non-token pieces.
        let mut piece = vocab::token(rank).to_vec();
        piece.extend_from_slice(vocab::token((rank * 31 + 17) % 100_256));
        piece.extend_from_slice(vocab::token((rank * 7 + 3) % 100_256));
        if piece.len() > 1 {
            check(&piece);
        }
    }
    for piece in [
        &b"ab"[..],
        b"\xff\xfe",
        b"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        b"\x00\x01\x02",
    ] {
        check(piece);
    }
}

#[test]
fn split_matches_fancy_regex_on_edge_cases() {
    for text in [
        "",
        " ",
        "  ",
        "a",
        "hello world",
        "  hello  world  ",
        "it's I'LL we've THEY'RE don't 'x 'S 'ſ 'll' ''",
        "x = foo(bar[1234567], \"baz\");\r\n\r\n\tnext  line\n",
        "\n\n\n",
        " \n \n ",
        "\t\t\tx",
        "a  \t  b",
        "1234567890 12 3",
        "٣٤٥٦ Ⅷ½ 1٣",
        "!!! ??? ...\n\n",
        " !!!\r\n\r\nx",
        "日本語のテキスト、です。",
        "emoji 😀😀 text",
        "e\u{301}e\u{301} combining",
        "\u{a0}\u{a0}x \u{85}\u{2028}y\u{3000}",
        "trailing spaces   ",
        "trailing newline\n",
        "a'b's'",
        "''''",
        "\u{0}\u{0}abc",
    ] {
        assert_same(text);
    }
}

#[test]
fn split_matches_fancy_regex_on_all_short_strings() {
    // Exhaustive over every string of length <= 3 from ALPHABET.
    let mut buf = String::new();
    for &a in ALPHABET {
        for &b in ALPHABET {
            for &c in ALPHABET {
                for text in [[a].as_slice(), &[a, b], &[a, b, c]] {
                    buf.clear();
                    buf.extend(text);
                    assert_eq!(fast_pieces(&buf), regex_pieces(&buf), "pieces of {buf:?}");
                }
            }
        }
    }
}

#[test]
fn split_matches_fancy_regex_on_random_strings() {
    // Deterministic xorshift: no extra dev-dependency, reproducible.
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..5_000 {
        let len = (next() % 48) as usize;
        let text: String = (0..len)
            .map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize])
            .collect();
        assert_same(&text);
    }
}

#[test]
fn split_matches_fancy_regex_for_every_char_in_context() {
    // Each scalar value, alone and next to every class the pattern treats
    // differently, in a few long strings (one regex run each).
    let chars: Vec<char> = (0..=0x10FFFF_u32).filter_map(char::from_u32).collect();
    for (before, after) in [
        ("", ""),
        (" ", "a"),
        ("'", "1"),
        ("a", " "),
        ("\n", "!"),
        ("1", "\r"),
    ] {
        let mut text = String::new();
        for &ch in &chars {
            text.push_str(before);
            text.push(ch);
            text.push_str(after);
        }
        assert_eq!(
            fast_pieces(&text),
            regex_pieces(&text),
            "context {before:?}/{after:?}"
        );
    }
}

#[test]
fn counts_match_tiktoken_on_repository_sources() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut stack = vec![repo.join("crates"), repo.join("docs"), repo.join("web/src")];
    let mut checked = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !matches!(
                    entry.file_name().to_str(),
                    Some("target" | "node_modules" | "dist")
                ) {
                    stack.push(path);
                }
            } else if kind.is_file() {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    assert_eq!(
                        count_str(&text),
                        oracle().encode_ordinary(&text).len() as i64,
                        "{}",
                        path.display()
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 100, "only {checked} files checked");
}
