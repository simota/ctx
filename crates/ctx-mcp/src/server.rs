use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::budget::*;
use crate::pack::*;
use crate::prompts::*;
use crate::protocol::*;
use crate::resources::*;
use crate::symbols::*;
use crate::tools::*;
use crate::types::*;
use crate::util::*;
use crate::where_q::*;
use crate::*;
use serde_json::{json, value::RawValue, Value};

pub(crate) struct Server {
    pub(crate) root: PathBuf,
    pub(crate) allow_outside_root: bool,
}

impl Server {
    pub(crate) fn new(opts: ServeOptions) -> Self {
        let root = absolute(&opts.root);
        let root = std::fs::canonicalize(&root).unwrap_or(root);
        Self {
            root,
            allow_outside_root: opts.allow_outside_root,
        }
    }

    pub(crate) fn handle(&self, req: Request) -> Response {
        let mut response = Response {
            jsonrpc: "2.0",
            id: req.id,
            result: None,
            error: None,
        };
        if req.jsonrpc != "2.0" {
            response.error = Some(RpcError {
                code: -32600,
                message: "invalid request".to_string(),
                data: None,
            });
            return response;
        }
        match req.method.as_str() {
            "initialize" => {
                response.result = Some(json!({
                    "capabilities": {"prompts": {}, "resources": {}, "tools": {}},
                    "protocolVersion": PROTOCOL_VERSION,
                    "serverInfo": {"name": "ctx", "version": "dev"},
                }));
            }
            "tools/list" => {
                response.result = Some(json!({"tools": tools()}));
            }
            "prompts/list" => {
                response.result = Some(json!({"prompts": list_prompts()}));
            }
            "prompts/get" => match get_prompt(req.params.as_deref()) {
                Ok(result) => response.result = Some(result),
                Err(err) => response.error = Some(err),
            },
            "resources/list" => {
                response.result = Some(json!({"resources": self.list_resources()}));
            }
            "resources/templates/list" => {
                response.result = Some(json!({"resourceTemplates": resource_templates()}));
            }
            "resources/read" => match self.read_resource(req.params.as_deref()) {
                Ok(result) => response.result = Some(result),
                Err(err) => response.error = Some(err),
            },
            "tools/call" => match self.call_tool(req.params.as_deref()) {
                Ok(result) => response.result = Some(result),
                Err(err) => response.error = Some(err),
            },
            _ => {
                response.error = Some(RpcError {
                    code: -32601,
                    message: format!("method not found: {}", req.method),
                    data: None,
                });
            }
        }
        response
    }

    pub(crate) fn call_tool(&self, raw: Option<&RawValue>) -> Result<Value, RpcError> {
        let params: CallParams = parse_json_opt(raw)?;
        let text = match params.name.as_str() {
            "ctx_pack" => self.run_pack(params.arguments.as_deref())?,
            "ctx_budget" => self.run_budget(params.arguments.as_deref())?,
            "ctx_where" => self.run_where(params.arguments.as_deref())?,
            "ctx_symbols" => self.run_symbols(params.arguments.as_deref())?,
            "ctx_skim" => self.run_skim(params.arguments.as_deref())?,
            "ctx_tree" => self.run_tree(params.arguments.as_deref())?,
            "ctx_focus" => return Ok(self.run_focus(params.arguments.as_deref())?),
            "ctx_digest" => return Ok(self.run_digest(params.arguments.as_deref())?),
            "ctx_roots_list" => self.run_roots_list(params.arguments.as_deref())?,
            _ => {
                return Ok(tool_error_result(&format!("unknown tool: {}", params.name)));
            }
        };
        Ok(json!({"content": [{"type": "text", "text": text}]}))
    }

    pub(crate) fn parse_pack_args(&self, raw: Option<&RawValue>) -> Result<PackArgs, RpcError> {
        let mut args: PackArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        validate_len("goal", &args.goal, MAX_GOAL_LEN)?;
        if args.path.is_empty() {
            args.path = ".".to_string();
        }
        if args.budget == 0 {
            args.budget = 50_000;
        }
        if args.budget < 0 || args.budget > MAX_BUDGET {
            return invalid_params_with_hint(
                &format!("budget must be between 0 and {MAX_BUDGET}"),
                "suggested: 50000",
            );
        }
        if args.format.is_empty() {
            args.format = "markdown".to_string();
        }
        Ok(args)
    }

    pub(crate) fn run_pack(&self, raw: Option<&RawValue>) -> Result<String, RpcError> {
        let args = self.parse_pack_args(raw)?;
        let root = self.resolve_path(&args.path)?;
        let mut files = collect_pack_files(&root).map_err(tool_error)?;
        if args.changed {
            files.clear();
        }
        for file in &mut files {
            file.tokens = count_file_tokens(&file.abs_path, file.size);
            file.symbols = ctx_symbols::extract(&file.abs_path).unwrap_or_default();
        }
        let plan = build_pack_plan(files, &args);
        match args.format.as_str() {
            "json" => render_pack_json(&plan, &args).map_err(tool_error),
            "plain" => render_pack_plain(&plan, &args),
            "xml" => Ok(render_pack_xml(&plan, &args)),
            _ => render_pack_markdown(&plan, &args),
        }
    }

    pub(crate) fn run_focus(&self, raw: Option<&RawValue>) -> Result<Value, RpcError> {
        let mut args: FocusArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        validate_len("anchor", &args.anchor, MAX_ANCHOR_LEN)?;
        if args.anchor.is_empty() {
            return Ok(tool_error_result("ctx_focus: 'anchor' is required"));
        }
        if args.path.is_empty() {
            args.path = ".".to_string();
        }
        if args.hops == 0 {
            args.hops = 1;
        }
        if args.hops < 0 || args.hops > MAX_HOPS {
            return invalid_params_with_hint(
                &format!("hops must be between 0 and {MAX_HOPS}"),
                "suggested: 1",
            );
        }
        if args.budget == 0 {
            args.budget = 8_000;
        }
        if args.budget < 0 || args.budget > MAX_BUDGET {
            return invalid_params_with_hint(
                &format!("budget must be between 0 and {MAX_BUDGET}"),
                "suggested: 8000",
            );
        }
        if args.format.is_empty() {
            args.format = "markdown".to_string();
        }

        let root = self.resolve_path(&args.path)?;
        let files = collect_pack_files(&root).map_err(tool_error)?;
        let focus_inputs = build_focus_inputs(&files);
        let result = match ctx_focus::pack(
            &focus_inputs,
            &args.anchor,
            &ctx_focus::ExpandOptions { hops: args.hops },
        ) {
            Ok(result) => result,
            Err(err) if err.candidates.len() > 1 => {
                let mut text = String::new();
                text.push_str(&format!(
                    "Multiple candidates for anchor {:?}:\n",
                    args.anchor
                ));
                for (i, candidate) in err.candidates.iter().enumerate() {
                    text.push_str(&format!(
                        "{}) {}:{} ({})\n",
                        i + 1,
                        candidate.path,
                        candidate.line,
                        candidate.kind
                    ));
                }
                text.push_str("\nRe-call ctx_focus with a more specific anchor (e.g. 'file.go:Symbol' or full repo-relative path).\n");
                return Ok(json!({"content": [{"type": "text", "text": text}]}));
            }
            Err(err) => return Err(tool_error(format!("anchor not found: {}", err.anchor))),
        };

        let mut files_by_path = BTreeMap::new();
        for mut file in files {
            file.tokens = count_file_tokens(&file.abs_path, file.size);
            files_by_path.insert(file.path.clone(), file);
        }

        let mut pack_files = Vec::new();
        let mut used = 0;
        for expanded in &result.files {
            let Some(file) = files_by_path.get(&expanded.path) else {
                continue;
            };
            if args.budget > 0 && used + file.tokens > args.budget {
                continue;
            }
            used += file.tokens;
            pack_files.push(file.clone());
        }

        let mut text = String::new();
        text.push_str(&format!(
            "# anchor={} origin={} hops={} files={} tokens={}/{}\n",
            args.anchor,
            result.anchor.origin_path,
            args.hops,
            pack_files.len(),
            used,
            args.budget
        ));
        if pack_files.len() <= 1 {
            if args.hops < MAX_HOPS {
                text.push_str(&format!(
                    "Note: only {} file in neighbourhood at hops={}. Try `ctx_focus {{\"anchor\":{},\"hops\":{}}}` for a wider view, or `ctx_where` with related symbols.\n",
                    pack_files.len(),
                    args.hops,
                    serde_json::to_string(&args.anchor).unwrap_or_else(|_| "\"\"".to_string()),
                    args.hops + 1
                ));
            } else {
                text.push_str(&format!(
                    "Note: only {} file in neighbourhood at hops={} (max). Call `ctx_where {{\"query\":{}}}` to discover related symbols.\n",
                    pack_files.len(),
                    args.hops,
                    serde_json::to_string(&args.anchor).unwrap_or_else(|_| "\"\"".to_string())
                ));
            }
        }
        text.push_str("## File contents\n\n");
        for file in &pack_files {
            write_pack_file_content(&mut text, file)?;
        }
        Ok(json!({"content": [{"type": "text", "text": text}]}))
    }

    pub(crate) fn run_digest(&self, raw: Option<&RawValue>) -> Result<Value, RpcError> {
        let mut args: DigestArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        validate_len("since", &args.since, MAX_SINCE_LEN)?;
        validate_len("cursor", &args.cursor, MAX_CURSOR_LEN)?;
        if args.page_size < 0 || args.page_size > MAX_PAGE_SIZE {
            return invalid_params_with_hint(
                &format!("page_size must be between 0 and {MAX_PAGE_SIZE}"),
                "suggested: 100",
            );
        }
        if args.path.is_empty() {
            args.path = ".".to_string();
        }
        if args.since.is_empty() {
            args.since = "7d".to_string();
        }
        if args.top == 0 {
            args.top = 10;
        }
        if args.top < 0 || args.top > MAX_TOP {
            return invalid_params_with_hint(
                &format!("top must be between 0 and {MAX_TOP}"),
                "suggested: 10",
            );
        }
        if digest_since_days(&args.since)? > 5 * 365 {
            return invalid_params_with_hint(
                "since must be ≤ 43800h0m0s",
                "suggested: '7d', '2w', or '1mo'",
            );
        }
        let root = self.resolve_path(&args.path)?;
        let _ = &args.format;
        if !root.join(".git").exists() {
            return Ok(tool_error_result("repository does not exist"));
        }
        Ok(tool_error_result(
            "ctx_digest: native repository digest is not implemented",
        ))
    }

    pub(crate) fn run_roots_list(&self, raw: Option<&RawValue>) -> Result<String, RpcError> {
        let _args: RootsListArgs = parse_json_opt(raw)?;
        let registry = load_roots()
            .map_err(|err| tool_error(format!("ctx_roots_list: load registry: {err}")))?;
        let mut roots = registry.roots;
        roots.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        if roots.is_empty() {
            return Ok("(no roots registered; run `ctx roots add <path>` or `ctx browse <path>` to register one)".to_string());
        }

        let current = canonicalize_for_compare(&self.root);
        let mut out = String::new();
        out.push_str("NAME\tPATH\tLAST_OPENED\tCURRENT\n");
        for root in roots {
            let last = root
                .last_opened_at
                .map(|dt| dt.to_string())
                .unwrap_or_else(|| "-".to_string());
            let marker = if current.as_ref().is_some_and(|current| {
                canonicalize_for_compare(Path::new(&root.path)).as_ref() == Some(current)
            }) {
                "*"
            } else {
                ""
            };
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                root.name, root.path, last, marker
            ));
        }
        Ok(out)
    }

    pub(crate) fn run_budget(&self, raw: Option<&RawValue>) -> Result<String, RpcError> {
        let mut args: BudgetArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        if args.path.is_empty() {
            args.path = ".".to_string();
        }
        if args.budget < 0 || args.budget > MAX_BUDGET {
            return invalid_params_with_hint(
                &format!("budget must be between 0 and {MAX_BUDGET}"),
                "suggested: 50000",
            );
        }
        let root = self.resolve_path(&args.path)?;
        let mut files = collect_budget_files(&root).map_err(tool_error)?;
        for file in &mut files {
            file.tokens = count_file_tokens(&file.abs_path, file.size);
        }
        format_budget_plan(&files, args.budget).map_err(tool_error)
    }

    pub(crate) fn run_where(&self, raw: Option<&RawValue>) -> Result<String, RpcError> {
        let mut args: WhereArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        validate_len("query", &args.query, MAX_QUERY_LEN)?;
        if args.path.is_empty() {
            args.path = ".".to_string();
        }
        if args.limit == 0 {
            args.limit = 10;
        }
        if args.limit < 0 || args.limit > MAX_LIMIT {
            return invalid_params("limit must be between 0 and 1000");
        }
        if args.format.is_empty() {
            args.format = "default".to_string();
        }
        let root = self.resolve_path(&args.path)?;
        let files = collect_where_files(&root).map_err(tool_error)?;
        let results = ctx_where::search_with_options(
            &files,
            &args.query,
            &ctx_where::Options {
                limit: args.limit,
                ..ctx_where::Options::default()
            },
        );
        format_where(&results, &args.format).map_err(tool_error)
    }

    pub(crate) fn run_symbols(&self, raw: Option<&RawValue>) -> Result<String, RpcError> {
        let mut args: SymbolsArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        validate_len("cursor", &args.cursor, MAX_CURSOR_LEN)?;
        if args.page_size < 0 || args.page_size > MAX_PAGE_SIZE {
            return invalid_params("page_size must be between 0 and 500");
        }
        if args.path.is_empty() {
            args.path = ".".to_string();
        }
        let root = self.resolve_path(&args.path)?;
        let entries = collect_symbol_entries(&root).map_err(tool_error)?;
        let (offset, size) = resolve_pagination(args.page_size, &args.cursor)?;
        let (page, next_offset) = apply_page_window(&entries, offset, size);

        let mut out = serde_json::Map::new();
        for entry in page {
            let syms: Vec<McpSymbol> = entry
                .symbols
                .iter()
                .map(|sym| McpSymbol {
                    name: sym.name.clone(),
                    kind: sym.kind.clone(),
                    line: sym.line,
                })
                .collect();
            out.insert(entry.path.clone(), json!(syms));
        }
        let mut body = serde_json::to_string_pretty(&out).map_err(tool_error)?;
        if let Some(footer) = pagination_footer(page.len(), entries.len(), next_offset) {
            body.push_str(&footer);
        }
        Ok(body)
    }

    pub(crate) fn run_skim(&self, raw: Option<&RawValue>) -> Result<String, RpcError> {
        let mut args: SkimArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        validate_len("lang", &args.lang, MAX_LANG_LEN)?;
        if args.path.is_empty() {
            return Err(tool_error("ctx_skim: 'path' is required"));
        }
        if args.budget == 0 {
            args.budget = 1000;
        }
        if args.budget < 0 || args.budget > MAX_BUDGET {
            return invalid_params_with_hint(
                &format!("budget must be between 0 and {MAX_BUDGET}"),
                "suggested: 1000",
            );
        }
        if args.unit.is_empty() {
            args.unit = "tokens".to_string();
        }
        if args.lang.is_empty() {
            args.lang = "auto".to_string();
        }

        let path = self.resolve_path(&args.path)?;
        let body =
            std::fs::read_to_string(&path).map_err(|err| tool_error(format!("skim: {err}")))?;
        let lang = detect_skim_lang(&path, &args.lang);
        let tokens = if args.unit == "chars" {
            body.chars().count() as i64
        } else {
            ctx_tokens::count_str(&body)
        };

        let tier = if args.tier.is_empty() {
            "full"
        } else {
            args.tier.as_str()
        };
        let overflow = if tokens > args.budget {
            " (over budget)"
        } else {
            ""
        };
        Ok(format!(
            "# tier={tier} tokens={tokens}/{}{overflow} path={} lang={lang}\n\n{body}",
            args.budget,
            path.display()
        ))
    }

    pub(crate) fn run_tree(&self, raw: Option<&RawValue>) -> Result<String, RpcError> {
        let mut args: TreeArgs = parse_json_opt(raw)?;
        validate_len("path", &args.path, MAX_PATH_LEN)?;
        validate_len("cursor", &args.cursor, MAX_CURSOR_LEN)?;
        if args.path.is_empty() {
            args.path = ".".to_string();
        }
        if args.depth < 0 || args.depth > MAX_DEPTH {
            return invalid_params_with_hint(
                &format!("depth must be between 0 and {MAX_DEPTH}"),
                "suggested: 5",
            );
        }
        if args.page_size < 0 || args.page_size > MAX_PAGE_SIZE {
            return invalid_params_with_hint(
                &format!("page_size must be between 0 and {MAX_PAGE_SIZE}"),
                "suggested: 100",
            );
        }
        if let Some(since) = &args.since {
            validate_len("since", since, MAX_SINCE_LEN)?;
        }
        if let Some(until) = &args.until {
            validate_len("until", until, MAX_SINCE_LEN)?;
        }

        let with_tokens = args.with_tokens.unwrap_or(true);
        let _with_git = args.with_git.unwrap_or(true);
        let _ = (&args.depth, &args.since, &args.until, &args.use_mtime);

        let root = self.resolve_path(&args.path)?;
        let mut files = collect_budget_files(&root).map_err(tool_error)?;
        let symbol_entries = if args.with_symbols {
            collect_symbol_entries(&root).map_err(tool_error)?
        } else {
            Vec::new()
        };
        let symbols_by_path: BTreeMap<String, Vec<McpSymbol>> = symbol_entries
            .into_iter()
            .map(|entry| {
                let symbols = entry
                    .symbols
                    .into_iter()
                    .map(|sym| McpSymbol {
                        name: sym.name,
                        kind: sym.kind,
                        line: sym.line,
                    })
                    .collect();
                (entry.path, symbols)
            })
            .collect();

        for file in &mut files {
            file.tokens = if with_tokens {
                count_file_tokens(&file.abs_path, file.size)
            } else {
                0
            };
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));

        let entries: Vec<TreeEntry> = files
            .into_iter()
            .map(|file| TreeEntry {
                symbols: symbols_by_path.get(&file.path).cloned().unwrap_or_default(),
                path: file.path,
                is_dir: false,
                tokens: file.tokens,
                git_status: String::new(),
            })
            .collect();

        if args.page_size == 0 && args.cursor.is_empty() {
            return serde_json::to_string_pretty(&entries).map_err(tool_error);
        }

        let (offset, size) = resolve_pagination(args.page_size, &args.cursor)?;
        let (page, next_offset) = apply_page_window(&entries, offset, size);
        let mut body = serde_json::to_string_pretty(page).map_err(tool_error)?;
        if let Some(footer) = pagination_footer(page.len(), entries.len(), next_offset) {
            body.push_str(&footer);
        }
        Ok(body)
    }

    pub(crate) fn resolve_path(&self, p: &str) -> Result<PathBuf, RpcError> {
        let path = Path::new(p);
        let target = if path.is_absolute() {
            absolute(path)
        } else {
            self.root.join(path)
        };
        let target = std::fs::canonicalize(&target).unwrap_or_else(|_| absolute(&target));
        if !self.allow_outside_root && target != self.root && !target.starts_with(&self.root) {
            return invalid_params_with_hint(
                "path outside server root",
                &format!(
                    "server root is {:?}; pass an absolute path inside it, or ask the operator to start `ctx mcp serve` with --allow-outside-root",
                    self.root.display().to_string()
                ),
            );
        }
        Ok(target)
    }

    pub(crate) fn list_resources(&self) -> Vec<Value> {
        resource_defs()
            .iter()
            .filter(|def| self.resolve_resource(def).is_some())
            .map(resource_value)
            .collect()
    }

    pub(crate) fn resolve_resource(&self, def: &ResourceDef) -> Option<PathBuf> {
        for candidate in def.files {
            let Ok(path) = self.resolve_path(candidate) else {
                continue;
            };
            let Ok(info) = std::fs::metadata(&path) else {
                continue;
            };
            if info.is_file() {
                return Some(path);
            }
        }
        None
    }

    pub(crate) fn read_resource(&self, raw: Option<&RawValue>) -> Result<Value, RpcError> {
        let params: ResourceReadParams = parse_json_opt(raw)?;
        if params.uri.is_empty() {
            return Err(tool_error("resources/read: 'uri' is required"));
        }

        for def in resource_defs() {
            if def.uri != params.uri {
                continue;
            }
            let Some(path) = self.resolve_resource(def) else {
                return Err(tool_error(format!(
                    "resource {}: backing file not found",
                    def.uri
                )));
            };
            let data = std::fs::read(&path)
                .map_err(|_| tool_error(format!("resource {}: read failed", def.uri)))?;
            return Ok(resource_read_result(
                def.uri.to_string(),
                def.mime_type,
                String::from_utf8_lossy(&data).into_owned(),
            ));
        }

        if params.uri.starts_with(FILE_RESOURCE_PREFIX) {
            return self.read_file_resource(&params.uri);
        }

        Err(tool_error(format!("unknown resource: {}", params.uri)))
    }

    pub(crate) fn read_file_resource(&self, uri: &str) -> Result<Value, RpcError> {
        let rel = uri.trim_start_matches(FILE_RESOURCE_PREFIX);
        if rel.is_empty() {
            return invalid_params_with_hint(
                "ctx://file/{path}: path segment is empty",
                "example: ctx://file/internal/mcp/server.go",
            );
        }
        if rel.len() > MAX_PATH_LEN {
            return invalid_params(&format!(
                "ctx://file/{{path}}: path exceeds max length {MAX_PATH_LEN}"
            ));
        }
        let path = self.resolve_path(rel)?;
        let info = std::fs::metadata(&path)
            .map_err(|_| tool_error(format!("resource {uri}: not a regular file")))?;
        if !info.is_file() {
            return Err(tool_error(format!("resource {uri}: not a regular file")));
        }
        let mut data =
            std::fs::read(&path).map_err(|_| tool_error(format!("resource {uri}: read failed")))?;
        let truncated = data.len() > MAX_FILE_RESOURCE_BYTES;
        if truncated {
            data.truncate(MAX_FILE_RESOURCE_BYTES);
        }
        let mut text = String::from_utf8_lossy(&data).into_owned();
        if truncated {
            text.push_str(&format!(
                "\n\n[truncated at {MAX_FILE_RESOURCE_BYTES} bytes; full size={} bytes. Use ctx_skim {{\"path\":{}}} or ctx_pack for a compressed view.]\n",
                info.len(),
                serde_json::to_string(rel).unwrap_or_else(|_| "\"\"".to_string())
            ));
        }
        Ok(resource_read_result(uri.to_string(), "text/plain", text))
    }
}
