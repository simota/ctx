// crates/ctx-contract/src/hash.rs
//
// Port of the sha256Hex helper in internal/contract/build.go. Kept in a
// dedicated module so other crates (e.g. ctx-contract-probe) can depend
// on it directly without pulling the rest of the contract surface.

use sha2::{Digest, Sha256};

/// Returns the lowercase hex-encoded SHA-256 of `b`. Output is 64 chars.
///
/// Matches Go's `hex.EncodeToString(sha256.Sum256(b)[:])` byte-for-byte.
pub fn sha256_hex(b: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_matches_known_digest() {
        // sha256("") known constant
        assert_eq!(
            sha256_hex(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn ascii_abc_matches_known_digest() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
