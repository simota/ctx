# Backlog — MCP Go->Rust byte-parity (one item per iteration)

Rules (the runner enforces; codex must obey):
- Implement EXACTLY the topmost unchecked `- [ ]` item, nothing else.
- Reuse native `ctx-*` crates (ctx-scan/where/symbols/tokens/relations/replay)
  — do NOT reimplement command logic; the MCP tool handlers wrap existing crates.
- Never edit Go (`internal/**`, `cmd/**`), `goal.md`, `verify.sh`, `run-loop.sh`,
  or any pinned `tests/` file. Never weaken/delete an existing parity case.
- The runner (not codex) checks items off AFTER verify + critic approve.
- If an item is not byte-parity-able, STOP and write a note to
  `crates/ctx-mcp/DEFERRED.md` instead of stubbing.

## Items (ordered: protocol skeleton → tools → resources/prompts → errors)
- [x] JSON-RPC framing + `initialize` handshake byte-parity (server info, capabilities, protocolVersion)
- [ ] `tools/list` byte-parity (tool schemas, order, descriptions)
- [ ] `tools/call` ctx_tree byte-parity (wraps ctx-scan tree)
- [ ] `tools/call` ctx_where byte-parity (wraps ctx-where)
- [ ] `tools/call` ctx_symbols byte-parity (wraps ctx-symbols)
- [ ] `tools/call` ctx_skim byte-parity
- [ ] `tools/call` ctx_focus byte-parity
- [ ] `tools/call` ctx_digest byte-parity
- [ ] `tools/call` ctx_budget byte-parity
- [ ] `tools/call` ctx_pack byte-parity
- [ ] `tools/call` ctx_roots_list byte-parity
- [ ] `resources/list` + `resources/templates/list` byte-parity
- [ ] `resources/read` byte-parity (incl. path-outside-root rejection)
- [ ] `prompts/list` + `prompts/get` byte-parity
- [ ] error paths byte-parity (unknown method, bad params, budget exceeded, too large)

(Refine this list during PHASE A once the harness corpus is authored; each
corpus case should map to a backlog item so DONE = backlog empty = all cases green.)
