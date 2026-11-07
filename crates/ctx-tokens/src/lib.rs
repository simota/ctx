// crates/ctx-tokens/src/lib.rs
//
// Token counting with the cl100k_base BPE encoder — mirrors internal/tokens.
//
// Counting matches Go's
// enc.Encode(text, nil, nil) semantics — tiktoken-rs `encode_ordinary`:
// special tokens are treated as ordinary text, no allowed-special /
// disallowed-special overrides.
//
// The count is `encode_ordinary(text).len()` computed without building the
// token vector: the text is split into the same pre-tokenizer pieces (see
// split.rs — a regex-free equivalent of the cl100k pattern, which dominated
// the encoder's run time), then each piece is one token when it is in the
// vocabulary and otherwise merged by `byte_pair_count`, a port of
// tiktoken-rs's own `_byte_pair_merge`. The vocabulary is the official
// cl100k_base.tiktoken file (sha256 223921b7…b2a7, the same bytes
// tiktoken-rs embeds), laid out as a static lookup table by build.rs (see
// vocab.rs), so the only run-time initialisation left is the split
// character classes.

mod split;
mod vocab;
mod vocab_hash;

use std::sync::OnceLock;

use vocab::Rank;

static CLASSES: OnceLock<split::Classes> = OnceLock::new();

fn classes() -> &'static split::Classes {
    CLASSES.get_or_init(split::Classes::new)
}

fn count_pieces(text: &str) -> usize {
    let mut parts = Vec::new();
    split::Pieces::new(text, classes())
        .map(|piece| {
            let bytes = piece.as_bytes();
            // Every single byte is in the vocabulary, so a piece that is
            // not a token has at least two bytes (`byte_pair_count`'s
            // precondition) — exactly tiktoken's encode_ordinary branch.
            if bytes.len() == 1 || vocab::rank(bytes).is_some() {
                1
            } else {
                byte_pair_count(bytes, &mut parts)
            }
        })
        .sum()
}

/// Number of tokens tiktoken's BPE merges `piece` (`len() >= 2`) into:
/// `byte_pair_split(piece, ranks).len()`, with the same merge loop as
/// tiktoken-rs 0.11's `_byte_pair_merge` (copied below step for step, so
/// ties and merge order are identical). `parts` is scratch space, reused
/// across pieces.
fn byte_pair_count(piece: &[u8], parts: &mut Vec<(usize, Rank)>) -> usize {
    debug_assert!(piece.len() > 1);
    let rank_of = |bytes: &[u8]| vocab::rank(bytes).unwrap_or(Rank::MAX);

    // (start, rank of the pair starting at start).
    parts.clear();
    let mut min_rank: (Rank, usize) = (Rank::MAX, usize::MAX);
    for i in 0..piece.len() - 1 {
        let rank = rank_of(&piece[i..i + 2]);
        if rank < min_rank.0 {
            min_rank = (rank, i);
        }
        parts.push((i, rank));
    }
    parts.push((piece.len() - 1, Rank::MAX));
    parts.push((piece.len(), Rank::MAX));

    let get_rank = |parts: &[(usize, Rank)], i: usize| {
        if (i + 3) < parts.len() {
            // `piece[i..i + 2]` above; the +3 is because parts[i + 1] is not
            // yet removed.
            rank_of(&piece[parts[i].0..parts[i + 3].0])
        } else {
            Rank::MAX
        }
    };

    while min_rank.0 != Rank::MAX {
        let i = min_rank.1;
        // Update parts[i] and parts[i - 1] before removing parts[i + 1].
        if i > 0 {
            parts[i - 1].1 = get_rank(parts, i - 1);
        }
        parts[i].1 = get_rank(parts, i);
        parts.remove(i + 1);

        min_rank = (Rank::MAX, usize::MAX);
        for (i, &(_, rank)) in parts[..parts.len() - 1].iter().enumerate() {
            if rank < min_rank.0 {
                min_rank = (rank, i);
            }
        }
    }
    // `windows(2)` over the parts: one token per adjacent pair.
    parts.len() - 1
}

/// Count the number of cl100k_base tokens in `text`.
///
/// Mirrors Go's `counter.CountString(text)` →
/// `len(enc.Encode(text, nil, nil))`.
pub fn count_str(text: &str) -> i64 {
    count_pieces(text) as i64
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
