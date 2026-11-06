// crates/ctx-tokens/src/vocab.rs
//
// The cl100k_base vocabulary as static data laid out by build.rs: a lookup
// is a hash probe over an embedded table, so there is nothing to initialise
// at run time and nothing on the heap. Exactness — every token maps to its
// rank and nothing else maps at all — is pinned by `vocabulary_matches_
// tiktoken` against tiktoken-rs's own decoder.

use crate::vocab_hash::{hash, home_slot, tag, EMPTY, RANK_MASK, SLOT_BITS};

/// cl100k_base token rank (tiktoken-rs's `Rank`).
pub(crate) type Rank = u32;

static TOKENS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cl100k_tokens.bin"));
static OFFSETS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cl100k_offsets.bin"));
static TABLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cl100k_table.bin"));

const SLOT_MASK: usize = (1 << SLOT_BITS) - 1;

#[inline]
fn word(bytes: &[u8], index: usize) -> u32 {
    let at = index * 4;
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4-byte word"))
}

/// Number of tokens in the vocabulary.
#[cfg(test)]
pub(crate) fn len() -> usize {
    OFFSETS.len() / 4 - 1
}

/// The bytes of the token with rank `rank` (`rank < len()`).
#[inline]
pub(crate) fn token(rank: Rank) -> &'static [u8] {
    let rank = rank as usize;
    &TOKENS[word(OFFSETS, rank) as usize..word(OFFSETS, rank + 1) as usize]
}

/// The rank of `bytes` if it is a token.
#[inline]
pub(crate) fn rank(bytes: &[u8]) -> Option<Rank> {
    let hash = hash(bytes);
    let tag = tag(hash);
    let mut slot = home_slot(hash);
    loop {
        let entry = word(TABLE, slot);
        if entry == EMPTY {
            return None;
        }
        if entry & !RANK_MASK == tag {
            let rank = entry & RANK_MASK;
            if token(rank) == bytes {
                return Some(rank);
            }
        }
        slot = (slot + 1) & SLOT_MASK;
    }
}
