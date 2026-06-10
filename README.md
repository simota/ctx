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
```

`ctx where` は、basename / path / symbol / content のシグナルを組み合わせてファイルをランキングします。
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

### 4. 小さく読む

```bash
ctx focus Pack --hops 1 --budget 8000
ctx skim crates/ctx-cli/src/main.rs --budget 800
ctx map --depth 2 --by tokens
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
| `ctx map` | token / file / symbol の heatmap を表示 |
| `ctx digest` | 最近の変更を要約 |
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

詳細は各コマンドの `--help` を確認してください。

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

既定では server を起動した root 配下だけにファイルアクセスを制限します。
複数 root を扱う場合は、意図を確認したうえで `--allow-outside-root` を使ってください。

## Browse UI

```bash
ctx browse .
ctx browse . --no-open --port 8765 --bind 127.0.0.1
```

`ctx browse` はローカルの Axum web server を起動します。
非 loopback address へ bind する場合は `--allow-nonlocal` が必要です。

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
