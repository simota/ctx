# ctx

**LLM に渡す文脈を、選り分ける。**

`ctx` は、リポジトリから AI に渡すコンテキストを作るための Rust 製 CLI / TUI / MCP server です。
ファイル構造、トークン数、Git 状態、シンボル、関連度を見ながら、必要なファイルだけを束ね、何を省いたかも記録します。

```bash
ctx --tokens --git --symbols .
ctx where "session login"
ctx where "session login" --format json | ctx pack --from-where --budget 30000 --out context.md
```

`tree` は構造を見る道具です。`rg` は文字列を探す道具です。
`ctx` は、LLM に渡す前に **select → order → budget → record** を行うための道具です。

## What It Does

- **Visualize**: ディレクトリ構造にトークン数、Git 状態、ファイル役割、シンボルを重ねて表示します。
- **Find**: 自然語クエリから関連ファイルをランキングします。
- **Pack**: 目的と予算に合わせて AI-ready な context bundle を生成します。
- **Verify**: `ctx pack` の contract manifest を使い、LLM 応答が渡した根拠に沿っているか検証します。
- **Replay**: pack snapshot を保存し、後から差分や再現性を確認します。
- **Browse / TUI / MCP**: Web UI、端末 UI、Claude Code / Cursor などからの MCP tool 呼び出しに対応します。

## Install

現時点ではソースからビルドします。通常のビルドには Rust toolchain が必要です。
Web frontend を再ビルドする場合だけ Node.js / pnpm が必要です。

```bash
git clone https://github.com/simota/ctx
cd ctx
make build
./bin/ctx --help
```

ユーザー環境へ入れる場合:

```bash
make install
ctx --help
```

`make install` は `cargo install --path crates/ctx-cli --locked --force` を実行し、`ctx` を Cargo の bin ディレクトリへ配置します。

## Quick Start

### 1. リポジトリを俯瞰する

```bash
ctx --tokens --git --symbols .
ctx --budget 50000 .
ctx --plan claude-pro --tokens .
```

`--plain` を付けると screen reader 向けの罫線なし出力になります。
`--json` を付けると構造化出力として処理できます。

### 2. 関連ファイルを探す

```bash
ctx where "authentication session"
ctx where "Pack" --format json --limit 5
ctx where --all "Auth Handler"
ctx where --regex 'http\.Handler' "server"
ctx where "ABテスト" --literal "ABテスト"
ctx where "session login" ~/src --repos-only --no-deps
```

`ctx where` は、basename / path / symbol / content のシグナルを組み合わせてファイルをランキングします。
`--literal` を付けると、クエリの小文字化や token 分割を使わず、大文字小文字を区別した exact パターンで候補を絞り込めます。
`--repos-only` を付けると、一致したファイルの代わりに、上位 `--limit` 件の一致を含むリポジトリを列挙します（ネストしたリポジトリはルートからの相対パス、ルート自身を含むリポジトリは `.`。`--json` で JSON 配列）。どのリポジトリにも属さない一致は最上位ディレクトリ名で表示し、そのようなルート直下のファイルは他に何も一致しない場合だけ `.` と表示します。
`--no-ignore`（または `--no-gitignore` / `--no-ctxignore`）はルートの ignore ファイルを無視し、`--no-deps` は `target/`・`vendor/`・`.venv/` などの依存・ビルド成果物ディレクトリも除外します。
逐語検索を最速で行いたい場合は `rg`、構文パターン検索をしたい場合は `ast-grep` が適しています。

### 3. AI に渡す bundle を作る

```bash
ctx pack . --goal "debug session expiration" --budget 30000 --out context.md
ctx pack . --changed --budget 20000
ctx pack . --diff HEAD~1..HEAD --layout unified
ctx where "session login" --format json | ctx pack --from-where --budget 30000 --out context.md
```

`ctx pack` は既定で末尾に `ctx:contract` manifest を埋め込みます。
不要な場合は `--no-contract` を指定します。

`--companions[=N]` を付けると、選んだファイル（`--changed` / `--from-where` / `--from-stdin` / `--from-mix`）と**過去のコミットで一緒に変更されてきたファイル**（テスト、スキーマ、設定など）を最大 N 件（既定 5）追加します。
companion は選んだファイルの後ろに置かれ、予算はまず選んだファイルに使われます。

```bash
ctx pack --changed --companions --budget 30000
```

#### 範囲を絞る・予算に収まらないファイルを扱う

```bash
ctx pack --include 'crates/ctx-web/**' --exclude tests --goal "routing"
ctx pack --include '*.{rs,toml}' --exclude '*_test.rs,benches'
ctx pack --goal "session expiry" --budget 20000 --overflow signatures
```

`--include` / `--exclude` は pack 対象のパスを glob で絞ります（繰り返し指定・カンマ区切り可、`--exclude` が優先）。
`*` はパスの 1 階層内、`**` は任意の階層、`{a,b}` は選択肢に一致し、`/` を含まないパターン（`*.rs`、`tests`）はどの階層のファイル名・ディレクトリ名にも一致します。ディレクトリに一致したパターンはその配下すべてを対象にします。
`--from-stdin` / `--from-mix` / `--from-where` で明示したファイルが除外された場合は、その旨が記録されます。

pack は**入れなかったファイルも記録します**。予算超過・見つからない・フィルタで除外されたファイルは Markdown の `## Omitted files`（plain は `-` 行、JSON は `skipped`、`--snapshot` の manifest は `skipped`）に理由付きで並び、予算超過分をすべて入れるのに必要な `--budget` の目安も表示します。goal に一致しなかったファイルは件数だけを示します。

`--overflow signatures` を付けると、関連度は高いが予算に入りきらないファイルを捨てる代わりに、その**宣言行（関数・型などのシグネチャ）を元の行番号付きで**入れます（入りきる場合のみ）。大きな中核ファイルの API を見せたまま予算を守れ、LLM は必要な箇所をパスと行番号で指定して追加で要求できます。既定は `--overflow drop`（従来通り除外）です。

### 4. 一緒に変わるファイルを知る

```bash
ctx cochange src/server.rs
ctx cochange src/auth/ --since 6mo --json
ctx cochange                      # 最も強く結合したファイルのペア
ctx cochange src/server.rs --format paths | ctx pack --from-stdin
```

`ctx cochange` は git 履歴から、指定ファイル（またはディレクトリ）と同じコミットで変更されてきたファイルを、共変更回数と confidence（そのファイルのコミットのうち相手も変更した割合）付きで並べます。
変更前に「ついでに直す必要がありそうなファイル」を把握したり、レビューで変更漏れを疑う材料にできます。50 ファイルを超える一括コミットは結合のノイズとして除外します。

### 5. 選んだファイルを保存して再利用する（mix）

```bash
ctx mix save "auth review" src/auth/ src/session.rs --goal "session expiry" --budget 30000
ctx where "session login" --json | ctx mix save login --from-where
ctx mix list
ctx mix show auth-review              # 現在の root に無いファイルは "(missing here)"
ctx mix add auth-review tests/session_test.rs
ctx mix drop auth-review src/auth/legacy.rs
ctx pack --from-mix auth-review --out context.md   # ID でも名前でも指定可
```

mix は「LLM に渡すファイルの順序付きリスト + goal + budget」を名前付きで保存したものです。
一度選んだファイル集合を、後のコミットでもそのまま `ctx pack --from-mix` で再 pack できます。
ディレクトリを渡すと pack が walk するファイル（ignore 適用後）に展開し、存在しないパスや root 外のパスはエラーにします。
`ctx browse` の Pins 画面と MCP `ctx_pack` の `mix` 引数も同じ store（`$XDG_STATE_HOME/ctx/mixes/` など）を使います。

`--from-stdin` / `--from-mix` で明示的に選んだファイルは、`--goal` に一致しなくても pack から外れません（goal は並び順にだけ効きます）。

### 6. 小さく読む

```bash
ctx focus Pack --hops 1 --budget 8000
ctx skim crates/ctx-cli/src/main.rs --budget 800
ctx map --depth 2 --by tokens
ctx map --depth 2 --by churn
ctx digest --since 7d
ctx noise --top 20
```

`focus` はシンボルやファイルを起点に周辺文脈を集めます。
`skim` は単一ファイルを token budget 内に縮約します。
`noise` は pack に入れるべきでない低信号ファイル候補を出します。

## Command Overview

| Command | Purpose |
|---|---|
| `ctx [path]` | リポジトリツリーを表示 |
| `ctx pack` | AI-ready context bundle を生成 |
| `ctx where` | 自然語クエリで関連ファイルを検索 |
| `ctx focus` | シンボル起点の mini-pack を生成 |
| `ctx skim` | 単一ファイルを予算内に縮約 |
| `ctx map` | token / file / symbol / churn の heatmap を表示 |
| `ctx digest [ROOT]` | 最近の変更を要約（サブディレクトリを渡すとその配下に限定） |
| `ctx cochange [PATH]` | 一緒に変更されてきたファイルを表示 |
| `ctx diff [ROOT]` | HEAD からの差分を表示（`--watch` で変更をストリーム） |
| `ctx mix` | 再利用できるファイル選択（mix）を保存・編集 |
| `ctx replay` | pack snapshot を管理 |
| `ctx noise` | context noise 候補を検出 |
| `ctx echo` | bundle が問いに答えられるか評価 |
| `ctx contract verify` | pack contract と応答を照合 |
| `ctx audit verify` | audit log を検証 |
| `ctx deps` / `ctx impact` | 依存関係と影響範囲を表示 |
| `ctx roots` | 既知の repository root を管理 |
| `ctx onboarding` | codebase onboarding 用の読み順を生成 |
| `ctx browse` | ローカル Web UI を起動 |
| `ctx tui` | 端末 UI を起動 |
| `ctx mcp serve` | MCP server を stdio で起動 |
| `ctx doctor` | 実行環境を診断 |

詳細は各コマンドの `--help` を確認してください（`pack` / `where` / `cochange` / `digest` / `mix` は利用可能なオプション一覧を表示します）。

`ctx diff [ROOT]` は staged / unstaged / untracked（ignore 対象外）の変更を raw patch として出力します。
`ctx diff [ROOT] --watch [--debounce 200ms] [--path PATH]...` は 100ms 間隔で監視し、変更が静止した時点の完全な patch snapshot（clean 遷移を含む）をイベント境界付きで出力します。

## MCP

`ctx mcp serve` は JSON-RPC stdio の MCP server として起動します。
Claude Code や Cursor などの MCP client から、context 生成や検索を tool として呼び出せます。

```json
{
  "mcpServers": {
    "ctx": {
      "command": "ctx",
      "args": ["mcp", "serve", "--root", "/path/to/repo"]
    }
  }
}
```

提供する tool: `ctx_pack`（`changed` / `companions` / `files` / `mix` / `include` / `exclude` / `overflow` 対応）、`ctx_where`、`ctx_focus`、`ctx_skim`、`ctx_budget`、`ctx_symbols`、`ctx_tree`、`ctx_digest`、`ctx_cochange`、`ctx_roots_list`。
編集前に `ctx_cochange {"anchor":"src/server.rs"}` で一緒に直すべきファイルを確認し、`ctx_pack {"changed":true,"companions":5}` で変更とその companion をまとめて渡す、といった使い方ができます。
`ctx_where` で見つけたファイルだけを渡すには `ctx_pack {"files":["src/a.rs","src/b.rs"]}`（指定順に pack し、budget 超過・root 外・存在しないファイルは skipped として報告）、保存済み mix は `ctx_pack {"mix":"auth-review"}` で pack できます。
ディレクトリ walk は `ctx_pack {"include":["src/**/*.rs"],"exclude":["tests"],"overflow":"signatures"}` のように CLI と同じ glob で絞り、予算に入らない関連ファイルをシグネチャだけで渡せます。

既定では server を起動した root 配下だけにファイルアクセスを制限します。
複数 root を扱う場合は、意図を確認したうえで `--allow-outside-root` を使ってください。

## Browse UI

```bash
ctx browse .
ctx browse . --no-open --port 8765 --bind 127.0.0.1
```

`ctx browse` はローカルの Axum web server を起動し、既定ではブラウザで表示します。
ブラウザ起動を抑止する場合は `--no-open` を指定してください。
非 loopback address へ bind する場合は `--allow-nonlocal` が必要です。
loopback に bind した server は DNS rebinding 対策として、`Host` が loopback IP
または `localhost` のリクエストにのみ応答します（それ以外は `403 forbidden_host`）。
表示された URL（`http://127.0.0.1:<port>/`）か `http://localhost:<port>/` で開いてください。
`--allow-nonlocal` で非 loopback に bind した場合、この `Host` 制限は適用されません。
Codespaces などの port forwarding や reverse proxy が独自の hostname を `Host` に載せる構成では
`403` になります（SSH の `-L` で `localhost` へ転送する場合は利用できます）。

Pins 画面（`#/pins`）では、ピン留めしたファイルを並び順のまま context pack にできます（goal / budget / Markdown・Plain を指定し、Copy または Download）。
同じ選択を mix として保存すると `ctx pack --from-mix <id>` や MCP から再利用でき、保存済み mix の Pack / ピンへの読み込み / 削除もここから行えます。
検索結果の「Pin all N results」で `where` の結果をまとめてピン留めできるので、検索 → ピン → pack をブラウザだけで完結できます。
状態を変更する API（`POST /api/mix`、`DELETE /api/mix/<id>`、`POST /api/pack`）は `Content-Type: application/json` を必須とし、ブラウザが cross-site と示すリクエスト（別 `Origin` / `Sec-Fetch-Site: cross-site`）を `403 forbidden_origin` で拒否します。

Frontend を変更した場合:

```bash
make web
make build
ctx browse .
```

## Configuration

設定例は [`ctx.toml.example`](ctx.toml.example) にあります。

```bash
cp ctx.toml.example ctx.toml
```

主な設定領域:

- `[ignore]`: `.gitignore` に加えて除外したい path pattern
- `[ai]`: tokenizer と既定 token budget
- `[security]`: secret scan / redact / strict offline
- `[pack]`: pack preset
- `[hooks]`: pre/post pack hook
- `[audit]`: audit log と query masking

## Security Model

`ctx` はローカルファーストのツールです。
通常の tree / where / pack はローカルファイルを読み、外部送信を前提にしません。

安全のため、次の挙動を持ちます。

- `.env*`, `*.pem`, `*.key`, `credentials.json`, `.netrc`, `.aws/` などの secret-bearing files を既定で避けます。
- secret scan / redact が有効な場合、検出した secret 候補を pack や MCP 応答から伏せます。
- MCP server は root jail を使い、既定で root 外へのアクセスを拒否します。
- audit log は opt-in で、必要に応じて raw / hash / mask の query handling を選べます。
- `--strict-offline` で外部ネットワークを使う可能性のある機能を無効化できます。

秘密情報を含む bundle を作る可能性がある場合は、出力を共有する前に必ず内容を確認してください。

## Project Structure

```text
crates/
  ctx-cli/             main CLI binary
  ctx-pack/            context bundle generation
  ctx-where/           relevance search
  ctx-symbols/         tree-sitter symbol extraction
  ctx-tokens/          token estimation
  ctx-web/             Axum web server and embedded SPA
  ctx-mcp/             MCP server
  ctx-tui/             terminal UI
  ctx-replay/          pack snapshot support
  ctx-contract/        pack-as-contract verification
  ctx-digest/          repository activity digest (CLI + MCP)
  ctx-git/             git history helpers (log, churn, co-change)
  ctx-mix/             saved file selections (mixes) shared by CLI, web and MCP
  ctx-*/               supporting libraries
web/                   Svelte / Vite frontend source
crates/ctx-web/dist/   embedded frontend assets
docs/                  static site and ADRs
ci/                    CI and parity helpers
loops/                 loop automation notes
```

The main binary is `crates/ctx-cli`.
The repository is intentionally organized as Rust crates instead of a single monolith, so individual capabilities can be tested and evolved independently.

## Development

```bash
make build      # build debug binary and copy it to ./bin/ctx
make release    # build optimized binary and copy it to ./bin/ctx
make check      # cargo check for the CLI crate
make test       # Rust test suites, including the Go parity oracle gate
make lint       # cargo clippy
make fmt        # cargo fmt
make web        # rebuild web frontend into crates/ctx-web/dist
make dev        # run Vite dev server only
make run ARGS="pack ."
```

Focused examples:

```bash
cargo test --manifest-path crates/ctx-pack/Cargo.toml
cargo test --manifest-path crates/ctx-web/Cargo.toml
cargo test --manifest-path crates/ctx-mcp/Cargo.toml
cargo clippy --manifest-path crates/ctx-cli/Cargo.toml --all-targets
```

`make test` builds or locates the frozen Go oracle used for parity checks.
The product binary itself is Rust; the oracle exists to prevent behavioral regressions during the migration history recorded in `docs/adr/0005-go-elimination-via-standalone-cli.md`.

## Design Notes

`ctx` is not trying to be a faster `rg`.

Use `rg` when you already know the exact text.
Use `ast-grep` when the query is a syntax pattern.
Use `ctx where` when you have a goal and need the files an LLM should see.
Use `ctx pack` when you want to preserve that selection as an auditable bundle.

The core idea is simple: better AI answers often come from a smaller, more deliberate context.
`ctx` makes that selection explicit.

## License

MIT.
