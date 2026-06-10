# Brand v2 — The Curator

> ctx は「AI に何を渡すか」を設計するツールであり、その本質は **「渡す前に、何を省くかを決める」** ことにある。  
> Brand v2 はこの reframing を骨格に据える。`tree` の系譜を借り続けるのをやめ、**LLM Context Curation** という未命名のカテゴリを ctx 自身が命名する側に回る。  
> Vision v1 (sumi 黒 + 朱印 + Noto Serif JP + 印刷物的静けさ) は捨てない。**その constraint の上に「学芸員 (curator) の手つき」を一枚重ねる evolution** として書き直す。

---

## Identity

### Name
- `ctx` — 変更なし。短い。`tree` / `eza` / `bat` と並んで打てる三文字。lowercase 固定。
- 文中での表記は ``` `ctx` ``` (バッククォート付き) を default、強調時のみ **ctx**。新カラータイトルや SaaS 風キャピタライズ ("CTX", "Ctx") は禁止。

### One-liner

| Surface | Copy |
|---|---|
| 日本語 (主) | **LLM に渡す文脈を、選り分ける。** |
| English (副) | **Curate the context you hand to AI — decide what to leave out.** |
| 短縮 (favicon alt, term-title 等) | LLM context curator. |

> Vision v1 の「AI に渡すコンテキストを、設計する。」は **設計** の語に滞留が出てきた (toolchain 全般に消費された)。`curate` / `選り分ける` は今のところ ctx の独占可能語。

### Category positioning — "LLM Context Curation"

- カテゴリ名: **LLM Context Curation** (英) / **文脈学芸員業** (日, 副次)
- 一行定義: *Selecting, ordering, and budgeting the minimum set of source files an LLM should see for a given goal — and recording what was deliberately left out.*
- 構成 4 動作: **select → order → budget → record**
- これは "repo packing" / "context engineering" / "prompt engineering" のどれとも違う:
  - repo packing は **全部入れる前提** の梱包
  - context engineering は **prompt 上のレイアウト** が主戦場
  - prompt engineering は **prompt 文** が単位
  - LLM Context Curation は **source-of-truth (= repo) から何を取り出さないか** が単位
- GEO 戦略: README / JSON-LD / og:description にこの語を一貫挿入する (Migration 9a, 9b 参照)。

---

## Spiritual ancestors

ctx の手つきは、これらの職能を継承している。装飾ではなく **判断の型** を借りる。

| Ancestor | 何を継承するか |
|---|---|
| 茶道の **mise en place** | 客が席に着く前に、出さない道具まで決め切る段取り。器の選定 = 文脈の選定 |
| 写本文化 (scriptorium) | 何を写し、何を省くかが知の継承を決めた。`--explain` は奥付に近い |
| 編集者 (editor) | 「載せない原稿を決める」のが仕事。掲載すれば質が落ちる原稿を断つ勇気 |
| 料理人の **materia prima** | 素材を見極めて切り出し、皿に乗せない部位を決める。骨やアラの扱い |
| 図書館員の **reference interview** | 利用者のゴールを聞き取り、関連文献を絞り込む。`--goal` の精神的祖先 |
| 学芸員 (museum curator) | 所蔵品から何を「展示しない」かが企画を決める。`Skipped` カテゴリの存在意義 |
| 索引製作者 (indexer) | 本文に手を入れずに、辿るための入口だけを設計する。`ctx where` / `ctx onboarding` |

> 7 個を並べたが運用上は **mise en place / 編集者 / 学芸員** の 3 つを最頻出のアナロジーに置く。それ以外は深掘り記事や READme 内のコラムでだけ召喚する。

---

## Palette

### 継承トークン (Vision v1 → v2 そのまま)

`docs/style.css` の `--lp-*` 15 トークンは **全て継承**。色設計の再起動はしない。`docs/index.html` / `docs/style.css` の token は変更不要。

| Token | Dark value | Light value | 用途 |
|---|---|---|---|
| `--lp-bg` | `#0e0c0a` | `#f5ead3` | 紙地 / 墨地 |
| `--lp-bg-elev` | `#171411` | `#ede0c4` | カード / code block |
| `--lp-bg-panel` | `#080705` | `#ebdfc2` | header / hero / footer |
| `--lp-fg` | `#ede2cf` | `#1c1612` | 本文 |
| `--lp-fg-dim` | `#a89a82` | `#5a4e3d` | 注釈 / 末梢 |
| `--lp-fg-strong` | `#ffffff` | `#0a0807` | 見出し |
| `--lp-accent` | `#e07a3a` | `#a83232` | 朱印 / CTA |
| `--lp-on-accent` | `#0e0c0a` | `#ffffff` | CTA 文字 |
| `--lp-border` | `#2a2420` | `#d4c5a0` | 罫線 |
| `--lp-link` | `#7fb6d6` | `#1c5a7a` | リンク |
| `--lp-success` | `#9ab86a` | `#4a6628` | ✓ / 完了 |

(残り 4 つの terminal syntax token はそのまま継承。)

### vermillion の方針 — **動かさない**

Compete agent からの懸念: 「`#e07a3a` は Anthropic の terracotta と色相が近接、誤読リスク」。

**判断: 色相は据え置く (`#e07a3a` のまま)。** 理由:
1. ctx の vermillion は **印影 (hanko)** という文脈で常に出現する。Anthropic は **brand logo / surface accent** で出る。**形状コンテキストが完全に異なる** ため、色単独の混同より、用途の違いの方が先に読まれる。
2. 色相を crimson (`#c0382f` 帯) に振ると、light theme の `--lp-accent` (`#a83232`) と区別がつかなくなり、dark/light で印影の "声" が変わる。Vision v1 の identity を壊す。
3. 識別は color ではなく **typography (Noto Serif JP) + symbol (印影 + 索引タブ) + voice (動詞語彙)** の 3 点で取る (= Anti-pattern 5 参照)。

WCAG 前提値 (継承 + 変更なしの確認):
- `--lp-accent` `#e07a3a` on `--lp-bg` `#0e0c0a` = **5.42:1** (AA pass for normal text, AAA for large text)
- `--lp-accent` `#a83232` on `--lp-bg` `#f5ead3` = **6.18:1** (AA pass, AAA large)
- 新規追加 token なし → contrast check 追加項目もなし

### 追加トークン: なし

Brand v2 は **palette を 1 個も足さない**。Curator の手つきは「選定の語彙」を増やすが、色は増やさない。装飾力で勝負しないという v1 の宣言を強化する。

---

## Typography

### 継承 (Vision v1)

| Role | Family | 用途 |
|---|---|---|
| `--font-display` | Noto Serif JP | h1 / h2 / `.pain-core` / `.solution-tagline` |
| `--font-sans` | Noto Sans JP, Inter | 本文 |
| `--font-en` | Inter | h3 / caption / button / table |
| `--font-mono` | JetBrains Mono | code / terminal / TOML / JSON |

Vision v1 ルール継承:
- 見出しは Noto Serif JP 700/600 + letter-spacing マイナス
- caption は 12px / 0.08em / uppercase
- mono は 13px / line-height 1.5
- glow / text-shadow / gradient 文字は禁止

### Curator 用に追加する 1 スタイル: `.article-quote`

Curator narrative (= 編集者 / 学芸員の引用文体) を表現する。**新規セクション用に 1 つだけ追加** (これも Migration 9c で scaffold のみ提示、適用は別 phase)。

```css
.article-quote {
  font-family: var(--font-display);          /* Noto Serif JP */
  font-size: clamp(1.0625rem, 1.6vw, 1.25rem);
  font-style: italic;                         /* 引用は斜体 = 編集者の声 */
  font-weight: 400;
  line-height: 1.8;
  color: var(--lp-fg);
  border-inline-start: 2px solid var(--lp-fg-dim);  /* accent ではなく dim — 主張しない */
  padding-inline-start: var(--sp-32);
  max-inline-size: 48ch;                      /* 視線が折り返さない長さ */
  margin-block: var(--sp-32);
}
.article-quote cite {
  display: block;
  margin-block-start: var(--sp-8);
  font-style: normal;
  font-family: var(--font-en);
  font-size: 13px;
  color: var(--lp-fg-dim);
  letter-spacing: 0.02em;
}
.article-quote cite::before { content: "— "; }
```

> 既存 `.solution-tagline` (border-inline-start 2px solid var(--lp-accent)) と意図的に差別化: accent 朱を使うのは "ctx 自身の宣言" の時だけ。引用は dim グレーで一段引く。

その他 typography 追加は **意図的にゼロ**。新フォントファミリも導入しない。

---

## Symbols

### 1. 朱印 (hanko) — Primary mark (継承)

- 既存ファイル: `docs/hanko.svg` / `docs/favicon.svg` / `docs/og.svg` 内 右側に配置
- 意味: **押した者の責任の所在**。「この pack は私が選定した」というサイン。`--explain` 出力の精神的シンボル。
- 運用ルール (継承):
  - 1 ページに 1 印が原則。連打しない
  - rotate しない (印は傾けない)
  - opacity は 0.9 (現状) より下げない
  - 色は `#e07a3a` (dark) / `#a83232` (light) のみ、グラデーション禁止
  - 印影の中心は必ず c/t/x の三段組み (装飾文字に置き換えない)

### 2. 索引タブ (index tab) — Canonical secondary mark (新規)

Curator direction を視覚化するため、**索引タブ** を canonical な副シンボルとして導入する。Brand layer に明示し、Migration phase で `docs/` に配備する。

**意味:** 本文に手を入れずに辿り口だけ作るのが索引製作者の仕事。`ctx where` / `ctx skim` / `ctx focus` の精神的シンボル。

**SVG 概念図 (12 x 24 viewBox 想定):**

```
┌────────┐     縦長の長方形 + 右端に直角の切り欠き
│        │     上辺 / 左辺 / 右上斜辺 / 右下水平で構成
│   c    │     中央に小さく "c" (Noto Serif JP, 朱)
│   t    │     用途に応じて t / x / where / skim 等の 1 語
│   x    │     朱の細線 stroke = 0.8px (印影より 3 倍細い)
│        │
│        │
│        │
└────────┘
```

詳細仕様:
- viewBox: `0 0 12 24` (高さ 2 倍の縦長)
- stroke: `var(--lp-accent)` / stroke-width: `0.8`
- fill: none (中身は透ける)
- 右上に 3px の切り欠き (タブの "ノッチ")
- 中の文字は 1 文字または短い英単語、`font-family: var(--font-display)` で 朱

**使う場所:**
- `docs/index.html` Features セクションの `feature-card` 左肩 (Migration 9b)
- README 内のセクション見出し近傍 (装飾 1 個まで)
- `ctx browse` UI のサイドバーでサブコマンド一覧の prefix (Migration 9d で議論)

**使わない場所:**
- Hero (Hero は印影 1 つで完結。索引タブを並べない)
- favicon (favicon は印影で固定)
- og 画像 (og は印影 + 大見出しで完結)
- 1 セクションに 3 個以上 (使いすぎは「装飾」になり Curator の声を裏切る)
- 印影の隣に並べる (印影の威厳が下がる)

> 索引タブの SVG ファイル (例 `docs/index-tab.svg`) を追加するかは Migration phase の判断。本 spec は概念図と運用ルールまで。

### 3. 補助エレメント (継承)

- `term-frame` (terminal 枠) — 黒地に朱の `term-dot` 1 個 + dim 2 個
- `code-block` — 罫線 1px + 内側 padding 16px
- `section-divider` — 1px の hr のみ

新規追加なし。

---

## Voice & Tone

### 5 原則 (Curator 固有)

1. **動詞で語る、形容詞で盛らない。** `blazing` / `powerful` / `revolutionary` は使わない。代わりに `選り分ける` / `見切る` / `省く` / `手渡す` を使う。
2. **省いた理由を必ず併記する。** `Skipped: 32 files (outside goal scope / too large / low relevance)` のような **削除理由の明示** を copy にも反映する。pack 出力の `--explain` と同じ手つきを LP コピーに持ち込む。
3. **誇張しない、句読点で間を取る。** 文末の `!` 禁止。`。` で止め、行間で説得する。広告 copy ではなく 印刷物の語り。
4. **読者を子ども扱いしない。** 専門語は専門語のまま置く (`tree-sitter` を「コード解析」に薄めない)。理解の段差は脚注ではなく次の段落で受ける。
5. **日本語と英語の両方を、どちらも省略形にしない。** Hero と og は両言語併記。略号 (LLM, MCP) はそのままで良いが marketese (AI-powered, next-gen) は削る。

### 動詞語彙 (Curator extension)

Vision v1 の語彙 (`設計する` / `渡す`) に以下を **追加** する。すべて Saga が specify したもの:

| 日本語 | 役割 |
|---|---|
| 選り分ける | 選定の主動詞 |
| 見切る | 「これ以上は無理」と判断する |
| 省く | 入れない判断 (Skipped に対応) |
| 手渡す | LLM への受け渡し (push でも feed でもない) |
| 読み返す | replay / snapshot diff の語感 |
| 整える | sort / format の語感 |
| 削る | 過剰な context の削減 |
| 残す | 残置の意図性を含む (token budget の選択肢として) |

### 禁則フレーズ (拡張)

Vision v1 で既に禁止: `blazing fast` / `next-gen` / `revolutionary`。これに以下を追加する:

| 禁則 | 理由 |
|---|---|
| `AI-powered` | 何でも powered になる時代、語が無意味化 |
| `Just dump everything` | ctx の anti-thesis そのもの |
| `Repository packaging` | 自分のカテゴリ (Context Curation) を捨てる |
| `One-click context` | 選定プロセスを隠す = ctx の手つきと逆 |
| `Beautiful UI` (= ctx 自身を形容して) | UI は手段。形容詞ではなく出力で示す |
| `量で勝負` / `全部貼り` | 排除メッセージとして anti-narrative を立てる |
| `vibe coding` | Curator は対極の職能。揶揄ではなく、棲み分けの宣言 |
| `Game-changing` / `Disruptive` | 静かな道具に騒がしい形容は不要 |

### Tone matrix

| Surface | Tone |
|---|---|
| README hero | 静か、宣言的、1 文 1 行 |
| LP hero | 静か、句読点で間を取る、英語サブで反復 |
| pack `--explain` 出力 | 機械的、定型、「なぜ含めたか」を 1 行で |
| Error / warning | 責めない、次の動作を提示 |
| FAQ | 質問を子ども扱いしない、不都合な事実 (CGO / ライセンス未定) を最初に書く |
| Twitter / SNS (将来) | 動詞だけで終わる短文。`選り分けた。` `省いた。` |

---

## Anti-patterns

Vision v1 の 5 個 (印影連打 / glow / 装飾フォント混在 / 賑やかしアイコン / 多色 accent) を **継承**。さらに Curator direction 固有を 3 個追加する。

| # | Don't | Why |
|---|---|---|
| 1 | 印影を 1 ページに複数押す (Vision v1) | 押印は「責任の所在」。連打すると意味が薄まる |
| 2 | gradient / glow / text-shadow を使う (Vision v1) | 印刷物的静けさを壊す |
| 3 | display font を 2 種以上混在させる (Vision v1) | 視線が "編集者の声" から外れる |
| 4 | 賑やかし用に絵文字 / 色付きアイコンを散らす (Vision v1) | 装飾力で勝負しない、出力の質で勝負する |
| 5 | 色を 3 色以上 accent として平等に使う (Vision v1) | accent = 朱、それ以外は dim/strong の二段だけで設計 |
| 6 | **選定プロセスを skip して結果だけ見せる UI を作る** (新規) | `Skipped: N files (reason)` を隠した瞬間に Curator ではなく packager になる。`--explain` を default に近づける方向は OK、削る方向は NG |
| 7 | **"AI 時代の tree" を主タグラインとして繰り返す** (新規) | 系譜継承は ancestor 説明としてのみ使う。primary 文言が他者カテゴリの参照だと、ctx 独自カテゴリ ("LLM Context Curation") が立たない |
| 8 | **`Skipped` を「失敗」「エラー」のトーンで描く** (新規) | Skipped は curator の積極的判断。✗ / red / warning icon ではなく、淡い dim で並べる。`Excluded` という英訳は避け `Skipped` / `Set aside` のニュアンスで |

---

## Migration Guide

既存 4 touchpoint について、Brand v1 → v2 の Before/After を具体的な edit instruction として書き出す。**本 spec は適用しない**。適用は別 phase の判断。

### (9a) README.md (`/Users/simota/repos/github.com/ctx/README.md`)

| # | 場所 | Before | After | 意図 |
|---|---|---|---|---|
| 1 | L3 hero タグライン | `> **AI 時代の `tree`.** コードベースをトークン・シンボル・Git 状態で可視化し、LLM に渡せる形へパックする CLI / TUI ツール。` | `> **LLM Context Curator.** リポジトリから AI に渡す束を選り分け、何を省いたかまで残す Rust 製 CLI / TUI / MCP server。` | tagline の主語を "tree の後継" から "curator" へ反転 |
| 2 | L17 文末 | `「AI に何を渡し、何を渡さないか」を設計するための道具です。` | `「AI に何を渡し、何を省いたか」を選り分け、記録する道具です。` | "渡さない" を "省いた" に置換、"記録する" を追加 |
| 3 | L22 章タイトル | `## なぜ `ctx` か` | (タイトルは維持) | — |
| 4 | L22 章の冒頭に 1 段落追加 | (なし) | `「全部貼って AI に任せる」では、AI の注意は分散し、回答精度は落ち、再現性も失われる。**ctx は LLM Context Curation という新しい職能 — 渡す前に何を省くかを決める仕事 — のための道具**である。`tree` が構造を見るための道具だったように、`ctx` は文脈を選り分けるための道具に位置する。` | カテゴリ命名 ("LLM Context Curation") を README に挿入 |
| 5 | L46 段落 ("`--engine rg` ...") の文末 | `…と誤読されるコストの方が高い。` | `…と誤読されるコストの方が高い。**ctx where は逐語 grep ではなく、LLM に手渡す前の選定動作 (curation step) として設計されている。**` | curation の語をここでも反復 |
| 6 | L953-963 ロードマップ | (現状: 機能ロードマップのみ) | 末尾に新項目追加: `- [ ] **category-establishment** — README / JSON-LD / og:description / 配布記事への "LLM Context Curation" 一貫挿入と外部リファレンスの確立` | カテゴリ命名を ship 可能タスクとして残す |
| 7 | L962 行付近 | `- [ ] `ctx ask "..."` — プロジェクト Q&A（要 AI 呼び出し、`--strict-offline` 尊重）` | `- [ ] `ctx ask "..."` — プロジェクト Q&A (curator-mode: 質問に対して **どの束を渡したか** と **なぜ他を省いたか** を必ず併記。`--strict-offline` 尊重)` | 将来機能まで curator 流儀で約束 |
| 8 | README 全体 | "コンテキストを設計する" 表記 (複数箇所) | 残しても良いが、**主要見出し級は "選り分ける" / "curate" に置換**、本文中の一般動詞としては "設計する" 残置可 | 主動詞の入替を見出しレイヤーから |

合計: **8 個の edit instruction** (実数で 5-10 個の枠内)。

### (9b) docs/index.html (`/Users/simota/repos/github.com/ctx/docs/index.html`)

| # | 場所 (file:line 風) | Before | After | 意図 |
|---|---|---|---|---|
| 1 | `index.html:6` `<title>` | `ctx — AI 時代の tree。LLM コンテキストを設計する Rust CLI` | `ctx — LLM Context Curator。AI に渡す束を選り分ける Rust CLI` | カテゴリ命名を title に |
| 2 | `index.html:7` `<meta name="description">` | `ctx はトークン数・Git 状態・シンボルでコードベースを可視化し、LLM に渡す最適な束をゴール起点でパックする Rust 製 CLI/TUI/MCP server。repomix の代替として Claude Code・Cursor から直接呼び出せる。` | `ctx は **LLM Context Curator** — リポジトリから AI に渡す束を選り分け、何を省いたかまで記録する Rust 製 CLI/TUI/MCP server。トークン数・Git 状態・シンボルでコードベースを可視化し、ゴール起点で curation できる。Claude Code・Cursor から MCP tool として呼び出せる。` | description にカテゴリ語 + curation 動詞 |
| 3 | `index.html:93` Hero h1 | `AI に渡すコンテキストを、設計する。` | `LLM に渡す文脈を、選り分ける。` | 主動詞反転 |
| 4 | `index.html:95-99` hero-sub | `ctx はトークン数・Git 状態・シンボルでコードベースを可視化し、ゴール起点で LLM に渡す最適なファイル群をパックする Rust CLI です。` + 英語 sub `Design the context you hand to AI — not just dump your entire repo.` | `ctx はトークン数・Git 状態・シンボルでコードベースを可視化し、**ゴールに照らして要るものを選り分け、省いたものも記録する** Rust CLI です。` + 英 sub `Curate the context you hand to AI — decide what to leave out, and keep a record of it.` | "パック" の語を temporarily 残しつつ、主動詞は "選り分け" に |
| 5 | `index.html:158` Pain heading | `あなたは今、LLM に何を渡しているか。` | `あなたは今、LLM に何を見せて、何を省いていますか。` | 受動 → 二択を迫る能動形 |
| 6 | `index.html:189` pain-core | `渡す量より、渡す質を設計する道具が、なかった。` | `何を省いたかが残る道具が、なかった。` | core message を curation の核心へ |
| 7 | `index.html:201` Solution heading | `tree は構造を見る道具。<br>ctx は「何を渡すか」を設計する道具。` | `tree は構造を見る道具。<br>ctx は「何を省くか」を選り分ける道具。` | solution heading の動詞反転 |
| 8 | `index.html:204-208` solution-tagline | `ctx はコードベースのトークン・シンボル・Git 状態を一覧し、自然語のゴールから関連ファイルをランキングして、LLM が消化できる最適な束にパックします。` | `ctx はコードベースを一覧し、自然語のゴールから要るファイルを選り分け、入らなかったものを `Skipped` 理由付きで残します。LLM に手渡す前に、curator の手つきを 1 段挟むための道具です。` | tagline に Skipped の言及を入れる |
| 9 | `index.html:429-430` mid-cta | `<p class="mid-cta-eyebrow">まず試してみる — 数ステップでインストール</p>` `<p>Rust toolchain があれば、そのまま動く。外部サービスへの送信はない。</p>` | `<p class="mid-cta-eyebrow">まずは 1 ファイル省くところから</p>` `<p>Rust toolchain でビルドでき、外部送信ゼロ。`ctx noise` でノイズを 1 個削るところから始められます。</p>` | CTA の心理的ハードルを「省く 1 動作」に下げる |
| 10 | `index.html:599` FAQ 末尾に 1 問追加 | (現状 5 問) | 6 問目: `<details><summary>なぜ "Curator" なのか? — packer / packager と何が違う?</summary><p>"pack" / "package" は **全部入れる前提** の梱包動作です。ctx の主動詞は <strong>curate</strong> — 渡さないものを決める動作 — に置きます。`ctx pack --explain` の出力に必ず Skipped 理由が並ぶのはこのためで、curator が「展示しない理由」まで残すのと同じ精神です。`ctx` を `repomix の代替` ではなく `LLM Context Curator` カテゴリの第一実装と位置づけます。</p></details>` | カテゴリ命名を FAQ で defended |
| 11 | `index.html:783-831` FAQPage JSON-LD | (現状 5 QA) | 上記 #10 の Q&A を 6 つ目として追加。`mainEntity` 配列に同等の Question/Answer 構造を append | GEO: AI に引用される FAQ に curator 定義を残す |
| 12 | `index.html:723` SoftwareApplication JSON-LD `description` | `ctx is a Rust CLI/TUI/MCP server that visualizes codebase token counts, ...` | `ctx is an LLM Context Curator — a Rust CLI/TUI/MCP server that visualizes codebase token counts, Git state, and symbols, then curates a minimum, goal-relevant set of source files to hand to an LLM, recording what was deliberately skipped. ...` | カテゴリ語を JSON-LD `description` 頭に挿入 |
| 13 | `index.html:735-746` JSON-LD `keywords` | (現状 10 件) | 先頭に追加: `"LLM Context Curation"`, `"context curator"`, `"LLM コンテキスト 選り分け"` | GEO: AI engines に新カテゴリ語を露出 |

合計: **13 個の edit instruction** (実数で 6-10 個の枠を超過しているが、その多くが JSON-LD / FAQ 並びの低リスク追記で、本文 copy 改変は 1-9 の **9 個**)。

### (9c) docs/style.css (`/Users/simota/repos/github.com/ctx/docs/style.css`)

CSS は **色トークン微調整なし**。新規追加は 2 ブロックのみ。

**追加 1: `.article-quote`** (Typography セクションで定義したもの。Migration phase で末尾に追記する):

```css
/* Curator narrative quote — append after .uc-card block (around L403) */
.article-quote {
  font-family: var(--font-display);
  font-size: clamp(1.0625rem, 1.6vw, 1.25rem);
  font-style: italic;
  font-weight: 400;
  line-height: 1.8;
  color: var(--lp-fg);
  border-inline-start: 2px solid var(--lp-fg-dim);
  padding-inline-start: var(--sp-32);
  max-inline-size: 48ch;
  margin-block: var(--sp-32);
}
.article-quote cite {
  display: block;
  margin-block-start: var(--sp-8);
  font-style: normal;
  font-family: var(--font-en);
  font-size: 13px;
  color: var(--lp-fg-dim);
  letter-spacing: 0.02em;
}
.article-quote cite::before { content: "— "; }
```

**追加 2: `.index-tab` utility class** (索引タブを Feature card 左肩に置くための装飾)。SVG ファイルを別途追加するか CSS で擬似的に作る選択肢があるが、ここでは **後者: CSS のみで pure な索引タブ** を提示する (画像追加なし = Curator らしい "省く" 選択):

```css
/* Index-tab badge — for feature cards / section anchors */
.index-tab {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  inline-size: 14px;
  block-size: 22px;
  background: transparent;
  border: 0.8px solid var(--lp-accent);
  border-end-end-radius: 0;
  border-end-start-radius: 0;
  font-family: var(--font-display);
  font-size: 10px;
  font-weight: 700;
  color: var(--lp-accent);
  letter-spacing: 0;
  line-height: 1;
  vertical-align: -3px;
  margin-inline-end: var(--sp-8);
}
/* Notch on the top-right corner — 3px diagonal cut */
.index-tab::after {
  content: '';
  position: absolute;
  top: -1px;
  right: -1px;
  inline-size: 4px;
  block-size: 4px;
  background: var(--lp-bg);
  border-inline-start: 0.8px solid var(--lp-accent);
  border-block-end: 0.8px solid var(--lp-accent);
  transform: rotate(45deg) translate(1px, -2px);
}
@media (prefers-reduced-motion: reduce) {
  /* No motion in this class; nothing to disable. */
}
```

使い方 (Migration 9b で `index.html` に挿入する場合):

```html
<div class="feature-card" role="listitem">
  <h3><span class="index-tab" aria-hidden="true">c</span>ctx where — ゴール起点の検索</h3>
  ...
</div>
```

**変更しないもの:**
- 既存 `--lp-*` 全 15 トークン
- 既存 component class すべて
- shadow / spacing / radius / motion duration

### (9d) web/src/app.css (`/Users/simota/repos/github.com/ctx/web/src/app.css`)

**変更不要 — 触らない。** 理由:

1. `web/` 配下の Svelte UI は **機能 UI** (ctx browse の閲覧画面) であり、`docs/` の **marketing surface** とは責任分界が異なる。Brand v2 の主戦場は marketing surface (= ユーザが ctx を最初に知る場所) であり、ユーザが既に道具として手に取った後の UI に同じ brand 圧をかける必要はない。
2. `web/src/app.css` は 15 テーマの token system (`--ctx-*` prefix) を独立に持つ。これは「閲覧者が自分の好みの作業環境を選べる」自由度であり、marketing brand から切り離す方が ctx の自分らしさに合う。Curator は展示室で大声を出さない。
3. `web/src/app.css` の token と `docs/style.css` の `--lp-*` token は **prefix で完全分離** 済み。token drift のリスクはなく、片方を編集してももう片方に影響しない。この分離を Brand v2 でも維持する。

> #TODO(agent): もし将来 `web/` の Welcome ページや About ダイアログを足す場合のみ、Brand v2 のタグラインを単発で使う余地がある。その時は本 spec の Identity 節を参照する。

---

## 締めに

Brand v2 は v1 を否定しない。v1 が用意した sumi 黒・朱印・Noto Serif JP・印刷物的静けさは、Curator という職能を演じるための舞台装置として **ちょうど良かった**。Brand v2 がしたのは、その舞台に立っているのは **誰なのか** を名指したことだけだ。

ctx は道具である前に、職能の名前である。**LLM Context Curator** — 渡す前に、何を省くかを決める人。
