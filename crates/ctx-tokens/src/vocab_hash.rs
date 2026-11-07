// crates/ctx-tokens/src/vocab_hash.rs
//
// The hash and slot layout of the cl100k_base lookup table. Shared verbatim
// by build.rs (which lays the table out) and vocab.rs (which probes it), so
// the two can never disagree.

/// The table has `1 << SLOT_BITS` slots (load factor 100_256 / 2^18 ≈ 0.38,
/// so most probes end at the first slot; 2^17 slots measured ~6% more
/// instructions per count).
pub const SLOT_BITS: u32 = 18;
/// Low bits of a slot: the token's rank (`< 2^17`; 100_256 tokens).
pub const RANK_BITS: u32 = 17;
pub const RANK_MASK: u32 = (1 << RANK_BITS) - 1;
/// A slot no token occupies. Its rank bits (`RANK_MASK`) exceed every rank,
/// so no occupied slot can equal it.
pub const EMPTY: u32 = u32::MAX;

const K: u64 = 0x517c_c1b7_2722_0a95;

/// A multiplicative (Fx-style) hash of a byte string, 8 bytes per round.
#[inline]
pub fn hash(bytes: &[u8]) -> u64 {
    let mut h = (bytes.len() as u64).wrapping_mul(K);
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        let word = u64::from_le_bytes(chunk.try_into().expect("8-byte chunk"));
        h = (h.rotate_left(5) ^ word).wrapping_mul(K);
    }
    let rest = chunks.remainder();
    if !rest.is_empty() {
        let mut buf = [0u8; 8];
        buf[..rest.len()].copy_from_slice(rest);
        h = (h.rotate_left(5) ^ u64::from_le_bytes(buf)).wrapping_mul(K);
    }
    h
}

/// First slot probed for `hash` (the well-mixed high bits).
#[inline]
pub fn home_slot(hash: u64) -> usize {
    (hash >> (64 - SLOT_BITS)) as usize
}

/// The `32 - RANK_BITS` tag bits stored beside the rank, from bits the slot
/// index does not use; a probe compares bytes only when the tag matches.
#[inline]
pub fn tag(hash: u64) -> u32 {
    ((hash >> 20) as u32) << RANK_BITS
}
