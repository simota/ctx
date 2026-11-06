// Thin wrapper over ctx_pack::assemble — the renderer itself was moved
// VERBATIM into crates/ctx-pack/src/assemble.rs so the ctx-tui `p` action
// can share it (see crates/ctx-tui/TUI_DEFERRED.md). The Native* names are
// re-exported aliases to keep braid/focus/exec/why call sites unchanged.
// CLI output through this path is byte-parity-gated against the Go oracle.

use super::*;

pub(crate) use ctx_pack::assemble::{
    build_pack_contract as build_native_pack_contract, is_default_breakdown, is_zero_i64,
    PackFile as NativePackFile, ReplayHeader as NativeReplayHeader,
};

pub(crate) fn render_native_pack(
    args: &PackArgs,
    files: &[NativePackFile],
    replay_header: Option<&NativeReplayHeader>,
) -> Result<String, String> {
    render_native_pack_with_skipped(args, files, &[], replay_header)
}

/// [`render_native_pack`] that also records the files the pack left out.
pub(crate) fn render_native_pack_with_skipped(
    args: &PackArgs,
    files: &[NativePackFile],
    skipped: &[ctx_pack::assemble::SkippedFile],
    replay_header: Option<&NativeReplayHeader>,
) -> Result<String, String> {
    let opts = ctx_pack::assemble::RenderOptions {
        format: args.format.clone(),
        goal: args.goal.clone(),
        budget: args.budget,
        explain: args.explain,
        no_metadata: args.no_metadata,
        no_paths: args.no_paths,
        frontmatter: args.frontmatter.clone(),
        plain_file_contents: args.plain_file_contents,
        // CLI flag is --no-contract; the library option is positive.
        contract: !args.no_contract,
    };
    ctx_pack::assemble::render_with_skipped(&opts, files, skipped, replay_header)
}
