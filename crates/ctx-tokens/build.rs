// crates/ctx-tokens/build.rs
//
// Lays the cl100k_base vocabulary out at compile time so the encoder needs no
// start-up work: decoding the 100_256 base64 lines into a hash map took
// ~20 ms (and ~7 MB of heap) in every process that counted a token.
//
// Outputs (in OUT_DIR, embedded with include_bytes! by src/vocab.rs):
//   cl100k_tokens.bin   every token's bytes, concatenated in rank order
//   cl100k_offsets.bin  u32 LE start offset of each rank, plus the end
//   cl100k_table.bin    u32 LE open-addressing (linear probing) slots of
//                       `tag | rank`, laid out by src/vocab_hash.rs

#[path = "src/vocab_hash.rs"]
mod vocab_hash;

use base64::Engine as _;

fn main() {
    let source = "assets/cl100k_base.tiktoken";
    println!("cargo:rerun-if-changed={source}");
    println!("cargo:rerun-if-changed=src/vocab_hash.rs");
    let text = std::fs::read_to_string(source).expect("read cl100k_base.tiktoken");

    let mut tokens: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();
    for (expected_rank, line) in text.lines().enumerate() {
        let (token, rank) = line
            .split_once(' ')
            .expect("cl100k_base line is `<base64> <rank>`");
        let rank: usize = rank.parse().expect("cl100k_base rank is an integer");
        // Ranks are the line numbers, so a rank indexes `offsets` directly.
        assert_eq!(rank, expected_rank, "cl100k_base ranks are contiguous");
        let token = base64::engine::general_purpose::STANDARD
            .decode(token)
            .expect("cl100k_base token is valid base64");
        offsets.push(u32::try_from(tokens.len()).expect("blob fits u32"));
        tokens.extend_from_slice(&token);
    }
    let count = offsets.len();
    offsets.push(u32::try_from(tokens.len()).expect("blob fits u32"));
    assert!(count < vocab_hash::RANK_MASK as usize, "ranks fit the slot");

    let slots = 1usize << vocab_hash::SLOT_BITS;
    let mask = slots - 1;
    let mut table = vec![vocab_hash::EMPTY; slots];
    for rank in 0..count {
        let bytes = &tokens[offsets[rank] as usize..offsets[rank + 1] as usize];
        let hash = vocab_hash::hash(bytes);
        let mut slot = vocab_hash::home_slot(hash);
        while table[slot] != vocab_hash::EMPTY {
            let other = (table[slot] & vocab_hash::RANK_MASK) as usize;
            let other = &tokens[offsets[other] as usize..offsets[other + 1] as usize];
            assert_ne!(other, bytes, "cl100k_base tokens are distinct");
            slot = (slot + 1) & mask;
        }
        table[slot] = vocab_hash::tag(hash) | rank as u32;
    }

    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let le = |words: &[u32]| {
        words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect::<Vec<u8>>()
    };
    std::fs::write(out.join("cl100k_tokens.bin"), &tokens).expect("write tokens");
    std::fs::write(out.join("cl100k_offsets.bin"), le(&offsets)).expect("write offsets");
    std::fs::write(out.join("cl100k_table.bin"), le(&table)).expect("write table");
}
