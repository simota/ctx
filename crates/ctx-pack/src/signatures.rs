// crates/ctx-pack/src/signatures.rs
//
// Signature digests for `ctx pack --overflow signatures` (and the MCP
// `ctx_pack` `overflow` argument): when a relevant file does not fit the
// remaining budget in full, the pack can carry its declaration lines
// instead of dropping it, so the model still sees the file's API surface
// and can ask for the full text by path and line.
//
// The digest is language-agnostic: it takes the source line of every
// extracted symbol (plus continuation lines while parentheses are open,
// for multi-line parameter lists) and prefixes each with its 1-based line
// number in the full file.

/// Longest run of continuation lines kept for one declaration.
const MAX_CONTINUATION_LINES: usize = 8;

/// Render the signature digest of `content` from its symbols' 1-based
/// declaration lines. Returns `None` when there is nothing to show (no
/// symbol lands on a line of the file).
pub fn signature_digest(
    path: &str,
    content: &str,
    symbol_lines: &[i64],
    full_tokens: i64,
) -> Option<String> {
    signature_digest_with_lines(path, content, symbol_lines, full_tokens).map(|(digest, _)| digest)
}

/// [`signature_digest`], plus the 1-based numbers of the source lines it
/// shows (declarations and their continuation lines), in order — what the
/// pack contract records for a signatures-only file.
pub fn signature_digest_with_lines(
    path: &str,
    content: &str,
    symbol_lines: &[i64],
    full_tokens: i64,
) -> Option<(String, Vec<usize>)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut wanted: Vec<usize> = symbol_lines
        .iter()
        .filter(|line| **line >= 1 && (**line as usize) <= lines.len())
        .map(|line| *line as usize - 1)
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    if wanted.is_empty() {
        return None;
    }
    let mut out = format!(
        "[ctx: signatures only - {path} is {full_tokens} tokens and did not fit the budget; \
         numbers are line numbers in the full file; ask for a body as {path}:START-END]\n"
    );
    let mut next_free = 0usize;
    let mut shown = Vec::new();
    for start in wanted {
        if start < next_free {
            continue; // already printed as a continuation line
        }
        let mut depth: i64 = 0;
        let mut idx = start;
        loop {
            let line = lines[idx].trim_end();
            out.push_str(&format!("{}: {}\n", idx + 1, line));
            shown.push(idx + 1);
            depth += paren_delta(line);
            idx += 1;
            if depth <= 0 || idx >= lines.len() || idx - start > MAX_CONTINUATION_LINES {
                break;
            }
        }
        next_free = idx;
    }
    Some((out, shown))
}

fn paren_delta(line: &str) -> i64 {
    line.chars().fold(0, |acc, c| match c {
        '(' => acc + 1,
        ')' => acc - 1,
        _ => acc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_declaration_lines_with_their_numbers() {
        let src = "use std::io;\n\npub struct Store {\n    dir: String,\n}\n\nimpl Store {\n    pub fn open(dir: &str) -> Self {\n        todo!()\n    }\n}\n";
        let digest = signature_digest("src/store.rs", src, &[3, 8], 120).unwrap();
        assert!(digest.starts_with("[ctx: signatures only - src/store.rs is 120 tokens"));
        assert!(digest.contains("3: pub struct Store {\n"));
        assert!(digest.contains("8:     pub fn open(dir: &str) -> Self {\n"));
        assert!(!digest.contains("todo!"));
    }

    #[test]
    fn follows_multi_line_parameter_lists() {
        let src = "fn build(\n    a: i32,\n    b: i32,\n) -> i32 {\n    a + b\n}\n";
        let digest = signature_digest("a.rs", src, &[1], 50).unwrap();
        assert!(digest.contains("1: fn build(\n2:     a: i32,\n3:     b: i32,\n4: ) -> i32 {\n"));
        assert!(!digest.contains("a + b"));
    }

    #[test]
    fn reports_the_source_lines_it_shows() {
        let src = "fn build(\n    a: i32,\n) -> i32 {\n    a\n}\nfn other() {}\n";
        let (_, lines) = signature_digest_with_lines("a.rs", src, &[6, 1], 50).unwrap();
        assert_eq!(lines, vec![1, 2, 3, 6]);
    }

    #[test]
    fn ignores_out_of_range_and_duplicate_lines() {
        let src = "def a():\n    pass\n";
        let digest = signature_digest("a.py", src, &[1, 1, 0, 99], 10).unwrap();
        assert_eq!(digest.matches("1: def a():").count(), 1);
        assert!(signature_digest("a.py", src, &[99], 10).is_none());
        assert!(signature_digest("a.py", src, &[], 10).is_none());
    }
}
