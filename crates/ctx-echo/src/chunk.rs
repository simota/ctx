// crates/ctx-echo/src/chunk.rs
//
// Port of internal/echo/chunk.go. The chunking pipeline is:
//
//   1. extract_file_blocks(body) — split on `### path` headings,
//      respecting fenced-code state so heading-shaped lines inside
//      code blocks don't fool the splitter.
//   2. strip_fences(block) — remove the leading ```lang and trailing
//      ``` lines that pack writer wraps each file in.
//   3. chunk_<strategy>(path, lines) — strategy-specific cut.
//   4. tokenize each chunk body so BM25 doesn't re-tokenize on every
//      query term.
//
// PARITY NOTE — bufio.Scanner cap: Go uses bufio.Scanner with a 32 MiB
// max line cap. If a single line exceeds 32 MiB the scanner returns
// false and the rest of the body is silently dropped. In practice no
// pack body has such lines (we cap pack output at ~10 MiB total) but
// we document the divergence: the Rust splitter uses `str::lines()`
// which has no cap.

use crate::tokenize::tokenize;
use crate::types::{Chunk, ChunkStrategy};
use once_cell::sync::Lazy;
use regex::Regex;

/// One `### path` section from the pack body. Mirrors Go's
/// `fileBlock`.
#[derive(Debug, Default)]
struct FileBlock {
    path: String,
    lines: Vec<String>,
}

/// Parse the pack markdown and yield one FileBlock per `### <path>`
/// heading. The body of each block ends at the next `###` heading or
/// `## ` section heading (e.g. `## Warnings`).
fn extract_file_blocks(body: &str) -> Vec<FileBlock> {
    let mut blocks: Vec<FileBlock> = Vec::new();
    let mut current: Option<FileBlock> = None;
    let mut in_fence = false;
    let mut fence_tag: String = String::new();

    for line in body.split('\n') {
        let trim = line.trim();

        // Detect fence toggles so heading-looking lines inside code
        // aren't misinterpreted as new file sections.
        if trim.starts_with("```") {
            if !in_fence {
                in_fence = true;
                fence_tag = trim.to_string();
            } else if trim == "```" || trim == fence_tag {
                in_fence = false;
                fence_tag.clear();
            }
        }

        if !in_fence {
            if line.starts_with("### ") {
                // New file section.
                if let Some(c) = current.take() {
                    blocks.push(c);
                }
                let path = line.strip_prefix("### ").unwrap_or("").trim().to_string();
                current = Some(FileBlock {
                    path,
                    lines: Vec::new(),
                });
                continue;
            }
            if line.starts_with("## ") && current.is_some() {
                // Section break ends file contents block.
                blocks.push(current.take().unwrap());
                continue;
            }
        }

        if let Some(c) = current.as_mut() {
            c.lines.push(line.to_string());
        }
    }

    if let Some(c) = current.take() {
        blocks.push(c);
    }
    blocks
}

/// Remove the leading ```lang and trailing ``` lines from a file
/// block body. The pack writer always wraps file contents in a single
/// fenced code block, so we strip exactly one pair when present.
fn strip_fences(lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut skip_next_fence = false;

    for (i, line) in lines.iter().enumerate() {
        let trim = line.trim();
        if i == 0 && trim.starts_with("```") {
            skip_next_fence = true;
            continue;
        }
        if skip_next_fence && trim == "```" {
            skip_next_fence = false;
            continue;
        }
        out.push(line.clone());
    }

    // Trim leading/trailing blank lines.
    let lead = out.iter().take_while(|l| l.trim().is_empty()).count();
    out.drain(..lead);
    while !out.is_empty() && out[out.len() - 1].trim().is_empty() {
        out.pop();
    }
    out
}

/// Match the start of a likely symbol definition. Mirrors `symbolBoundary`
/// in Go verbatim. Static regex compiled once per process.
static SYMBOL_BOUNDARY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?:\s{0,4})(?:func\s|type\s|class\s|function\s|def\s|fn\s|struct\s|enum\s|impl\s|export\s+(?:default\s+)?(?:function|class|const|interface|type|enum)\s|interface\s|public\s+(?:class|interface|enum|record)\s|private\s+(?:class|interface|enum)\s|module\s|trait\s)",
    )
    .expect("symbol boundary regex must compile")
});

/// Mirrors `echo.ChunkPack`. Splits the pack body into Chunks per the
/// requested strategy, then tokenizes each chunk's body so BM25 can
/// run length-normalised scoring in one pass.
pub fn chunk_pack(body: &str, strategy: ChunkStrategy, fixed_size: i32) -> Vec<Chunk> {
    let mut blocks = extract_file_blocks(body);
    if blocks.is_empty() {
        // No `### path` headings found — treat the whole body as one
        // anonymous file so paragraph chunking still produces output.
        // Matches Go's `blocks = []fileBlock{{path: "", lines:
        // strings.Split(body, "\n")}}`.
        blocks.push(FileBlock {
            path: String::new(),
            lines: body.split('\n').map(String::from).collect(),
        });
    }

    let mut chunks: Vec<Chunk> = Vec::new();
    for b in blocks {
        let lines = strip_fences(&b.lines);
        if lines.is_empty() {
            continue;
        }
        match strategy {
            ChunkStrategy::Fixed => {
                chunks.extend(chunk_fixed(&b.path, &lines, fixed_size));
            }
            ChunkStrategy::Symbol => {
                chunks.extend(chunk_symbol(&b.path, &lines));
            }
            ChunkStrategy::Paragraph => {
                chunks.extend(chunk_paragraph(&b.path, &lines));
            }
        }
    }

    // Tokenise + cache TokenLen.
    for c in chunks.iter_mut() {
        c.tokens = tokenize(&c.body);
        c.token_len = c.tokens.len() as i32;
    }
    chunks
}

fn chunk_paragraph(path: &str, lines: &[String]) -> Vec<Chunk> {
    let mut out: Vec<Chunk> = Vec::new();
    let mut buf: Vec<String> = Vec::new();
    let mut start: i32 = 1;

    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            if !buf.is_empty() {
                out.push(Chunk {
                    source_path: path.to_string(),
                    line_start: start,
                    line_end: i as i32, // i is 0-based; previous content ended at i (1-based).
                    body: buf.join("\n"),
                    tokens: Vec::new(),
                    token_len: 0,
                });
                buf.clear();
            }
            start = (i as i32) + 2;
            continue;
        }
        buf.push(line.clone());
    }
    // Flush trailing buffer.
    if !buf.is_empty() {
        out.push(Chunk {
            source_path: path.to_string(),
            line_start: start,
            line_end: lines.len() as i32,
            body: buf.join("\n"),
            tokens: Vec::new(),
            token_len: 0,
        });
    }
    out
}

fn chunk_symbol(path: &str, lines: &[String]) -> Vec<Chunk> {
    let mut out: Vec<Chunk> = Vec::new();
    let mut buf: Vec<String> = Vec::new();
    let mut start_idx: i32 = 0;

    let flush =
        |buf: &mut Vec<String>, out: &mut Vec<Chunk>, path: &str, start_idx: i32, end: i32| {
            if buf.is_empty() {
                return;
            }
            out.push(Chunk {
                source_path: path.to_string(),
                line_start: start_idx + 1,
                line_end: end,
                body: buf.join("\n"),
                tokens: Vec::new(),
                token_len: 0,
            });
            buf.clear();
        };

    for (i, line) in lines.iter().enumerate() {
        if SYMBOL_BOUNDARY.is_match(line) && !buf.is_empty() {
            flush(&mut buf, &mut out, path, start_idx, i as i32);
            start_idx = i as i32;
        }
        buf.push(line.clone());
    }
    flush(&mut buf, &mut out, path, start_idx, lines.len() as i32);
    out
}

fn chunk_fixed(path: &str, lines: &[String], size: i32) -> Vec<Chunk> {
    let size = if size <= 0 { 40 } else { size } as usize;
    let mut out: Vec<Chunk> = Vec::new();
    let n = lines.len();
    let mut i = 0usize;
    while i < n {
        let end = (i + size).min(n);
        out.push(Chunk {
            source_path: path.to_string(),
            line_start: (i as i32) + 1,
            line_end: end as i32,
            body: lines[i..end].join("\n"),
            tokens: Vec::new(),
            token_len: 0,
        });
        i += size;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_strategy_emits_ceil_chunks() {
        let body = "### a.go\n```\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n```\n";
        let chunks = chunk_pack(body, ChunkStrategy::Fixed, 3);
        // 10 lines / 3 = 4 chunks (3+3+3+1)
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].line_start, 1);
        assert_eq!(chunks[0].line_end, 3);
        assert_eq!(chunks[3].line_start, 10);
        assert_eq!(chunks[3].line_end, 10);
    }

    #[test]
    fn paragraph_strategy_splits_on_blank() {
        let body = "### a.go\n```\nalpha line one\nalpha line two\n\nbeta line\n```\n";
        let chunks = chunk_pack(body, ChunkStrategy::Paragraph, 0);
        // Two paragraphs after strip_fences trims fences + blank lines.
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].body.contains("alpha line one"));
        assert!(chunks[1].body.contains("beta line"));
    }

    #[test]
    fn anonymous_pack_when_no_heading() {
        let body = "no headings\njust text\n\nanother paragraph";
        let chunks = chunk_pack(body, ChunkStrategy::Paragraph, 0);
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].source_path, "");
    }
}
