// crates/ctx-tokens/src/lib.rs
//
// Token counting with the cl100k_base BPE encoder — mirrors internal/tokens.
//
// The encoder table is loaded once per process via OnceLock (mirrors Go's
// sync.Once in getSharedEncoder). Counting matches Go's
// enc.Encode(text, nil, nil) semantics — tiktoken-rs `encode_ordinary`:
// special tokens are treated as ordinary text, no allowed-special /
// disallowed-special overrides.
//
// The count is `encode_ordinary(text).len()` computed without building the
// token vector: the text is split into the same pre-tokenizer pieces (see
// split.rs — a regex-free equivalent of the cl100k pattern, which dominated
// the encoder's run time), then each piece is one token when it is in the
// vocabulary and otherwise merged with tiktoken-rs's own `byte_pair_split`.
// The vocabulary is the official cl100k_base.tiktoken file (sha256
// 223921b7…b2a7, the same bytes tiktoken-rs embeds); loading only the rank
// map — not tiktoken-rs's decoder, sorted-token table and per-thread regex
// clones — also makes the one-time initialisation cheaper.

mod split;

use std::sync::OnceLock;

use base64::Engine as _;
use rustc_hash::FxHashMap;
use tiktoken_rs::Rank;

const CL100K_BASE: &str = include_str!("../assets/cl100k_base.tiktoken");

struct Encoder {
    ranks: FxHashMap<Vec<u8>, Rank>,
    classes: split::Classes,
}

static ENCODER: OnceLock<Encoder> = OnceLock::new();

fn get_encoder() -> &'static Encoder {
    ENCODER.get_or_init(|| {
        let mut ranks = FxHashMap::default();
        ranks.reserve(100_256);
        for line in CL100K_BASE.lines() {
            let (token, rank) = line
                .split_once(' ')
                .expect("cl100k_base line is `<base64> <rank>`");
            let token = base64::engine::general_purpose::STANDARD
                .decode(token)
                .expect("cl100k_base token is valid base64");
            let rank: Rank = rank.parse().expect("cl100k_base rank is an integer");
            ranks.insert(token, rank);
        }
        Encoder {
            ranks,
            classes: split::Classes::new(),
        }
    })
}

impl Encoder {
    fn count(&self, text: &str) -> usize {
        split::Pieces::new(text, &self.classes)
            .map(|piece| {
                let bytes = piece.as_bytes();
                // Every single byte is in the vocabulary, so a piece that is
                // not a token has at least two bytes (`byte_pair_split`'s
                // precondition) — exactly tiktoken's encode_ordinary branch.
                if bytes.len() == 1 || self.ranks.contains_key(bytes) {
                    1
                } else {
                    tiktoken_rs::byte_pair_split(bytes, &self.ranks).len()
                }
            })
            .sum()
    }
}

/// Count the number of cl100k_base tokens in `text`.
///
/// Mirrors Go's `counter.CountString(text)` →
/// `len(enc.Encode(text, nil, nil))`.
pub fn count_str(text: &str) -> i64 {
    get_encoder().count(text) as i64
}

/// Count cl100k_base tokens for the content of a file.
///
/// Returns `Ok(count)` on success, `Err` if the file cannot be read or is
/// not valid UTF-8.  Mirrors Go's `counter.CountFile(path)`.
pub fn count_file(path: &str) -> Result<i64, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("read {path}: {err}"))?;
    let text = std::str::from_utf8(&bytes).map_err(|err| format!("utf8 {path}: {err}"))?;
    Ok(count_str(text))
}

/// Rough size-based estimate: 4 bytes per token, matching Go's EstimateBySize.
/// Use this only as a fallback when the real encoder is unavailable or the
/// file cannot be decoded as UTF-8.
pub fn estimate_by_size(size: i64) -> i64 {
    (size / 4).max(1)
}

#[cfg(test)]
mod tests;
