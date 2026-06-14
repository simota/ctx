// ctx HTTP API — types mirror Sherpa §3 spec. Builder serves /api/* on the
// same origin as the SPA (embedded). In dev, Vite proxies /api → :8080.

export interface Symbol {
  name: string;
  kind: string;
  line: number;
}

export interface TreeNode {
  path: string;
  name: string;
  is_dir: boolean;
  size: number;
  tokens?: number;
  lines?: number;
  role?: string;
  git?: string;
  updated_at?: number;
  symbols?: Symbol[];
  children?: TreeNode[];
}

export interface TreeResponse {
  // Requested sub-tree's path relative to the served project (typically '.').
  root: string;
  // Absolute filesystem path of the served project. Used for "Copy Full Path"
  // affordances; clients should not concatenate it with anything they did not
  // get from the same /api/tree response.
  abs_root: string;
  tree: TreeNode;
  total: number;
}

export interface FileResponse {
  path: string;
  size: number;
  lines: number;
  tokens: number;
  git?: string;
  role?: string;
  symbols?: Symbol[];
  content: string;
  truncated: boolean;
  lang?: string;
}

export interface WhereMatch {
  line: number;
  column: number;
  type: string;
  text: string;
}

export interface WhereResult {
  path: string;
  score: number;
  reason: string;
  matches: WhereMatch[];
}

export interface WhereResponse {
  query: string;
  results: WhereResult[];
}

export interface BudgetItem {
  path: string;
  tokens: number;
  reason?: string;
  group?: string;
}

export interface BudgetResponse {
  budget: number;
  used: number;
  included: BudgetItem[];
  excluded: BudgetItem[];
}

export interface SymbolsResponse {
  path: string;
  files: Record<string, Symbol[]>;
}

// /api/dir response — directory overview ("package readme" style).
// `path` is root-relative ('' for root); `name` is basename (or '.' for root).
// `tokens` / `file_count` / `dir_count` are recursive totals. `git` is an
// optional aggregated summary of working-tree status across descendants.
// `readme` is the truncated readme content (≤64KB) when one is detected and
// `readme_path` is its root-relative location for breadcrumb display.
export interface DirChild {
  path: string;
  name: string;
  is_dir: boolean;
  size: number;
  tokens?: number;
  git?: string;
}

export interface DirGitSummary {
  modified?: number;
  added?: number;
  deleted?: number;
  untracked?: number;
}

export interface DirResponse {
  path: string;
  name: string;
  tokens: number;
  file_count: number;
  dir_count: number;
  git?: DirGitSummary;
  readme?: string;
  readme_path?: string;
  children: DirChild[];
}

export interface ApiError {
  error: { code: string; message: string };
}

// ---------------------------------------------------------------------------
// fetch wrappers
// ---------------------------------------------------------------------------

export class ApiCallError extends Error {
  code: string;
  status: number;
  constructor(status: number, code: string, message: string) {
    super(message);
    this.status = status;
    this.code = code;
    this.name = 'ApiCallError';
  }
}

async function getJSON<T>(path: string): Promise<T> {
  const res = await fetch(path, { headers: { Accept: 'application/json' } });
  const text = await res.text();
  let body: unknown = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      throw new ApiCallError(res.status, 'parse_error', `Invalid JSON from ${path}`);
    }
  }
  if (!res.ok) {
    const err = (body as ApiError | null)?.error;
    throw new ApiCallError(res.status, err?.code ?? 'http_error', err?.message ?? res.statusText);
  }
  return body as T;
}

function qs(params: Record<string, string | number | boolean | undefined>): string {
  const u = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === '') continue;
    u.set(k, String(v));
  }
  const s = u.toString();
  return s ? `?${s}` : '';
}

export interface FetchTreeOpts {
  path?: string;
  depth?: number;
  tokens?: boolean;
  git?: boolean;
  role?: boolean;
  symbols?: boolean;
  since?: string;
  until?: string;
  useMtime?: boolean;
}

export function fetchTree(opts: FetchTreeOpts = {}): Promise<TreeResponse> {
  const { useMtime, ...rest } = opts;
  return getJSON<TreeResponse>(
    `/api/tree${qs({ ...rest, use_mtime: useMtime ?? undefined })}`,
  );
}

export interface FetchFileOpts {
  symbols?: boolean;
}

export function fetchFile(path: string, opts: FetchFileOpts = {}): Promise<FileResponse> {
  return getJSON<FileResponse>(
    `/api/file${qs({ path, symbols: opts.symbols })}`,
  );
}

export interface FetchWhereOpts {
  limit?: number;
  path?: string;
}

export function fetchWhere(query: string, opts: FetchWhereOpts = {}): Promise<WhereResponse> {
  return getJSON<WhereResponse>(`/api/where${qs({ q: query, ...opts })}`);
}

export interface FetchBudgetOpts {
  budget?: number;
  path?: string;
}

const DEFAULT_BUDGET = 50000;

export function fetchBudget(opts: FetchBudgetOpts = {}): Promise<BudgetResponse> {
  const budget = opts.budget ?? DEFAULT_BUDGET;
  return getJSON<BudgetResponse>(`/api/budget${qs({ ...opts, budget })}`);
}

export function fetchSymbols(path: string): Promise<SymbolsResponse> {
  return getJSON<SymbolsResponse>(`/api/symbols${qs({ path })}`);
}

// /api/definition response — symbol jump candidates. `column` is reserved by
// the wire format but the v1 backend never populates it (model.Symbol has no
// column field today). Frontend MUST ignore column and seek by line only.
export interface DefinitionCandidate {
  path: string;
  line: number;
  column?: number;
  kind: string;
  symbol_name: string;
  file_role?: string;
  file_tokens?: number;
}

export interface DefinitionResponse {
  name: string;
  candidates: DefinitionCandidate[];
}

export interface FetchDefinitionOpts {
  from?: string;
  kind?: string;
  signal?: AbortSignal;
}

// fetchDefinition: GET /api/definition with optional from/kind hints.
// AbortSignal support lets callers cancel in-flight hovers (the picker fires
// many requests as the user moves over the gutter).
export function fetchDefinition(
  name: string,
  opts: FetchDefinitionOpts = {},
): Promise<DefinitionResponse> {
  const url = `/api/definition${qs({ name, from: opts.from, kind: opts.kind })}`;
  return getJSONSig<DefinitionResponse>(url, opts.signal);
}

async function getJSONSig<T>(path: string, signal?: AbortSignal): Promise<T> {
  const res = await fetch(path, {
    headers: { Accept: 'application/json' },
    signal,
  });
  const text = await res.text();
  let body: unknown = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      throw new ApiCallError(res.status, 'parse_error', `Invalid JSON from ${path}`);
    }
  }
  if (!res.ok) {
    const err = (body as ApiError | null)?.error;
    throw new ApiCallError(res.status, err?.code ?? 'http_error', err?.message ?? res.statusText);
  }
  return body as T;
}

// fetchDir: root is fetched with `path=''` — the `qs` helper omits empty
// values, so the request becomes `/api/dir` and the server resolves to root.
export function fetchDir(path: string): Promise<DirResponse> {
  return getJSON<DirResponse>(`/api/dir${qs({ path })}`);
}

// ---------------------------------------------------------------------------
// relations — per-file import edges (in / out) for Go and JS/TS files.
// Unsupported extensions return empty arrays with HTTP 200 rather than 4xx.
// ---------------------------------------------------------------------------

export interface RelationItem {
  path: string;
}

export interface RelationsResponse {
  path: string;
  module_path?: string;
  imports: RelationItem[];
  importers: RelationItem[];
}

export function fetchRelations(path: string): Promise<RelationsResponse> {
  return getJSON<RelationsResponse>(`/api/relations${qs({ path })}`);
}

// ---------------------------------------------------------------------------
// tests — static test relevance plus existing Go coverprofile data.
// The backend never runs tests from this endpoint; coverage is present only
// when a coverprofile such as coverage.out exists in the served root.
// ---------------------------------------------------------------------------

export interface TestInsightFile {
  path: string;
  score: number;
  reasons?: string[];
  test_count?: number;
  matched_symbols?: string[];
}

export interface TestInsightSource {
  path: string;
  score: number;
  reasons?: string[];
  matched_symbols?: string[];
}

export interface TestCoverageRange {
  start: number;
  end: number;
}

export interface TestCoverageSummary {
  profile: string;
  total_stmts: number;
  covered_stmts: number;
  percent: number;
  uncovered_lines?: TestCoverageRange[];
}

export interface TestInsightResponse {
  path: string;
  kind?: string;
  tests: TestInsightFile[];
  sources?: TestInsightSource[];
  total_tests?: number;
  total_sources?: number;
  coverage?: TestCoverageSummary;
}

export function fetchTestInsights(path: string): Promise<TestInsightResponse> {
  return getJSON<TestInsightResponse>(`/api/tests${qs({ path })}`);
}

// ---------------------------------------------------------------------------
// evidence — replay-backed proof that the selected file was included in a
// previous context pack, plus stale/fresh status against the current worktree.
// ---------------------------------------------------------------------------

export interface EvidenceSnapshot {
  id: string;
  created_at: string;
  goal?: string;
  budget: number;
  used: number;
  format: string;
  status: 'fresh' | 'stale' | 'missing' | string;
  path: string;
  pack_sha256: string;
  current_sha256?: string;
  tokens: number;
  current_tokens?: number;
  token_delta?: number;
  relevance?: string;
  score?: number;
  reason?: string;
  message?: string;
}

export interface EvidenceResponse {
  path: string;
  status: 'fresh' | 'stale' | 'missing' | 'no-evidence' | 'no-store' | string;
  store_path?: string;
  total_snapshots: number;
  snapshots: EvidenceSnapshot[];
}

export function fetchEvidence(path: string, limit = 6): Promise<EvidenceResponse> {
  return getJSON<EvidenceResponse>(`/api/evidence${qs({ path, limit })}`);
}

export interface EvidenceVerifyRequest {
  pack: string;
  response: string;
  check_worktree?: boolean;
  no_symbols?: boolean;
  strict?: boolean;
}

export interface EvidenceVerifyViolation {
  kind: string;
  path?: string;
  line_start?: number;
  line_end?: number;
  symbol?: string;
  expected_sha256?: string;
  got_sha256?: string;
  source_line?: number;
  message?: string;
}

export interface EvidenceVerifyOK {
  kind: string;
  path?: string;
  line_start?: number;
  line_end?: number;
  symbol?: string;
  source_line?: number;
}

export interface EvidenceVerifyStaleFile {
  path: string;
  expected_sha256: string;
  got_sha256?: string;
  message?: string;
}

export interface EvidenceVerifyResponse {
  pack_file: string;
  schema_version: number;
  total_files_in_contract: number;
  references_found: number;
  violations: EvidenceVerifyViolation[];
  ok: EvidenceVerifyOK[];
  stale_files: EvidenceVerifyStaleFile[];
  repack_suggestions: string[];
  exit_code: number;
}

export function verifyEvidence(req: EvidenceVerifyRequest): Promise<EvidenceVerifyResponse> {
  return postJSON<EvidenceVerifyResponse>('/api/evidence/verify', req);
}

// ---------------------------------------------------------------------------
// git diff — line-level diff between a file at HEAD and the worktree.
// `lines` carries every diff line in order; `eq` lines have both old_num and
// new_num set, `add` lines have only new_num, `del` lines have only old_num.
// `binary` / `no_change` / `added` / `deleted` are mutually informative flags
// (binary trumps the others). `truncated` signals that the diff exceeded the
// server-side line cap.
// ---------------------------------------------------------------------------

export interface GitDiffLine {
  type: 'add' | 'del' | 'eq';
  text: string;
  old_num?: number;
  new_num?: number;
}

export interface GitDiffResponse {
  path: string;
  added?: boolean;
  deleted?: boolean;
  binary?: boolean;
  no_change?: boolean;
  truncated?: boolean;
  lines: GitDiffLine[];
}

export function fetchGitDiff(path: string): Promise<GitDiffResponse> {
  return getJSON<GitDiffResponse>(`/api/git/diff${qs({ path })}`);
}

// ---------------------------------------------------------------------------
// git file log — commit history for a single file.
// ---------------------------------------------------------------------------

export interface FileLogEntry {
  hash: string;       // short hash (7 chars)
  hash_full: string;  // full 40-char SHA
  author: string;
  author_email: string;
  subject: string;
  date: number;       // unix seconds
}

export interface FileLogResponse {
  path: string;
  commits: FileLogEntry[];
  truncated: boolean;
}

export function fetchFileLog(path: string, limit?: number): Promise<FileLogResponse> {
  return getJSON<FileLogResponse>(`/api/git/file-log${qs({ path, limit })}`);
}

// fetchFileCommitDiff: diff of `path` between commits `from` and `to`.
// Reuses GitDiffResponse since the wire format is identical.
export function fetchFileCommitDiff(
  path: string,
  from: string,
  to: string,
): Promise<GitDiffResponse> {
  return getJSON<GitDiffResponse>(`/api/git/commit-diff${qs({ path, from, to })}`);
}

// ---------------------------------------------------------------------------
// git log — repository-wide commit history (newest first). Reuses the
// FileLogEntry shape (the server emits the same fields); `truncated` signals
// more history exists beyond the requested window.
// ---------------------------------------------------------------------------

export interface RepoLogResponse {
  commits: FileLogEntry[];
  truncated: boolean;
}

export function fetchGitLog(limit?: number): Promise<RepoLogResponse> {
  return getJSON<RepoLogResponse>(`/api/git/log${qs({ limit })}`);
}

// git commit files — paths changed by a single commit, with status.
export type CommitFileStatus = 'added' | 'modified' | 'deleted';

export interface CommitFileEntry {
  status: CommitFileStatus;
  path: string;
  additions?: number;
  deletions?: number;
  binary?: boolean;
}

export interface CommitFilesResponse {
  hash: string;
  files: CommitFileEntry[];
}

export function fetchCommitFiles(hash: string): Promise<CommitFilesResponse> {
  return getJSON<CommitFilesResponse>(`/api/git/commit-files${qs({ hash })}`);
}

// git branches — local branches with their short target hash; `current` marks
// the branch HEAD points at.
export interface BranchEntry {
  name: string;
  hash: string;
  current?: boolean;
  subject?: string;
  date?: number; // unix seconds of the tip commit
  ahead?: number; // commits ahead of HEAD
  behind?: number; // commits behind HEAD
}

export interface BranchesResponse {
  branches: BranchEntry[];
}

export function fetchBranches(): Promise<BranchesResponse> {
  return getJSON<BranchesResponse>('/api/git/branches');
}

// git tags — newest first; `date` is unix seconds.
export interface TagEntry {
  name: string;
  hash: string;
  date?: number;
}

export interface TagsResponse {
  tags: TagEntry[];
}

export function fetchTags(): Promise<TagsResponse> {
  return getJSON<TagsResponse>('/api/git/tags');
}

// git worktrees — linked worktrees (incl. the main one). `branch` is null when
// detached or bare.
export interface WorktreeEntry {
  path: string;
  branch?: string | null;
  head?: string;
  bare?: boolean;
  detached?: boolean;
}

export interface WorktreesResponse {
  worktrees: WorktreeEntry[];
}

export function fetchWorktrees(): Promise<WorktreesResponse> {
  return getJSON<WorktreesResponse>('/api/git/worktrees');
}

// ---------------------------------------------------------------------------
// replay snapshots — mirror Go `replay.Manifest` + list summary.
// ---------------------------------------------------------------------------

export interface ReplayListItem {
  id: string;
  created_at: string; // RFC3339
  goal?: string;
  budget: number;
  used: number;
  format: string;
  preset?: string;
  ctx_version: string;
  file_count: number;
}

export interface ReplayListResponse {
  snapshots: ReplayListItem[];
  store_path: string;
}

export interface ReplayManifestEntry {
  path: string;
  sha256: string;
  tokens: number;
  relevance?: string;
  score?: number;
  reason?: string;
}

export interface ReplayManifestSkipped {
  path: string;
  reason: string;
}

export interface ReplayManifest {
  schema_version: number;
  id: string;
  created_at: string;
  ctx_version: string;
  goal?: string;
  budget: number;
  used: number;
  root: string;
  preset?: string;
  format: string;
  out_sha256?: string;
  entries: ReplayManifestEntry[];
  skipped?: ReplayManifestSkipped[];
}

export function fetchReplayList(): Promise<ReplayListResponse> {
  return getJSON<ReplayListResponse>('/api/replay/list');
}

export function fetchReplayShow(id: string): Promise<ReplayManifest> {
  return getJSON<ReplayManifest>(`/api/replay/show${qs({ id })}`);
}

// Diff of a snapshot vs the current working tree. The server returns changes
// sorted (modified > added > removed, then by |tokens_delta| DESC) and may
// truncate when `limit` is hit (signaled via `truncated: true`).
export interface ReplayDiffChange {
  path: string;
  kind: 'added' | 'modified' | 'removed';
  tokens_delta: number;
  base_tokens?: number;
  current_tokens?: number;
}

export interface ReplayDiffResponse {
  snapshot_id: string;
  snapshot_time: string;
  changes: ReplayDiffChange[];
  unchanged_count: number;
  total_token_delta: number;
  strict: boolean;
  truncated: boolean;
}

export interface FetchReplayDiffOpts {
  strict?: boolean;
  limit?: number;
}

export function fetchReplayDiff(
  id: string,
  opts: FetchReplayDiffOpts = {},
): Promise<ReplayDiffResponse> {
  return getJSON<ReplayDiffResponse>(
    `/api/replay/diff${qs({ id, strict: opts.strict, limit: opts.limit })}`,
  );
}

export interface ReplayVerifyRequest {
  id: string;
  response: string;
  check_worktree?: boolean;
  strict?: boolean;
}

export function verifyReplayResponse(req: ReplayVerifyRequest): Promise<EvidenceVerifyResponse> {
  return postJSON<EvidenceVerifyResponse>('/api/replay/verify', req);
}

// ---------------------------------------------------------------------------
// roots — registry of known project roots, plus a "spawn a child ctx browse
// for this root" handshake. The backend rejects /api/roots/open when the
// server is not bound to loopback (security), and rejects unregistered paths
// (so this endpoint can't become an arbitrary-process spawn vector).
// ---------------------------------------------------------------------------

export interface RootEntry {
  name: string;
  path: string;
  // RFC3339 timestamp. omitempty on the server, so undefined is possible
  // for legacy entries written before the field existed.
  added_at?: string;
  last_opened_at?: string;
}

export interface RootsListResponse {
  roots: RootEntry[];
  warning?: string;
}

export interface RootsOpenRequest {
  name?: string;
  path?: string;
}

export interface RootsOpenResponse {
  name: string;
  path: string;
  url: string;
  port: number;
}

// POST /api/roots — register (or refresh) a root. `path` is required; `name`
// is optional and defaults to basename(canonicalize(path)) on the server.
export interface RootsCreateRequest {
  name?: string;
  path: string;
}

export interface RootsCreateResponse {
  root: RootEntry;
}

export function listRoots(): Promise<RootsListResponse> {
  return getJSON<RootsListResponse>('/api/roots');
}

export async function openRoot(req: RootsOpenRequest): Promise<RootsOpenResponse> {
  return postJSON<RootsOpenResponse>('/api/roots/open', req);
}

export async function createRoot(req: RootsCreateRequest): Promise<RootsCreateResponse> {
  return postJSON<RootsCreateResponse>('/api/roots', req);
}

// DELETE /api/roots?name=NAME_OR_PATH. The server returns 204 on success and
// no body, so we resolve void on the happy path.
export async function deleteRoot(name: string): Promise<void> {
  const url = `/api/roots${qs({ name })}`;
  const res = await fetch(url, {
    method: 'DELETE',
    headers: { Accept: 'application/json' },
  });
  if (res.status === 204) return;
  const text = await res.text();
  let body: unknown = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      throw new ApiCallError(res.status, 'parse_error', `Invalid JSON from ${url}`);
    }
  }
  if (!res.ok) {
    const err = (body as ApiError | null)?.error;
    throw new ApiCallError(res.status, err?.code ?? 'http_error', err?.message ?? res.statusText);
  }
}

async function postJSON<T>(path: string, req: unknown): Promise<T> {
  const res = await fetch(path, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Accept: 'application/json',
    },
    body: JSON.stringify(req),
  });
  const text = await res.text();
  let body: unknown = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      throw new ApiCallError(res.status, 'parse_error', `Invalid JSON from ${path}`);
    }
  }
  if (!res.ok) {
    const err = (body as ApiError | null)?.error;
    throw new ApiCallError(res.status, err?.code ?? 'http_error', err?.message ?? res.statusText);
  }
  return body as T;
}
