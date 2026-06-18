# RFC: git log に「ファイル相関図」ビューを追加する

> Status: Proposal (提案のみ・実装なし)
> Author: Spark
> Date: 2026-06-18
> Recipe: `propose`

---

## 0. ユーザー問題（解決策名ではなく問題で命名）

**「どのファイルがいつも一緒に変わるのかが、コミット履歴を1件ずつ開かないと見えない」**

ctx の git log ビューは「時間軸（誰がいつ何を変えたか）」は `GitLogList.svelte` の commit graph で見せられるが、
「**構造軸**（ファイル同士の暗黙の結合）」を見せる手段がない。リポジトリに不慣れな開発者は
「この関数を直すと、ついでにどのファイルを直す羽目になるか」を、コミットを総当たりで開いて頭の中で集計している。
これは典型的な compensating behavior（手作業集計）であり、未充足ジョブの兆候。

### Job To Be Done（progress として framing）

> リポジトリに参加した開発者が、**変更の波及範囲を事前に把握し、自信を持って 1 ファイルに手を入れられる**ようになること。
> （activity「コミット履歴を見る」ではなく、progress「未知のコードベースで安全に変更できる」）

### Target Persona

**「Newcomer / Code Archaeologist」** — 既存リポジトリ（OSS への初回コントリビュート、引き継ぎ案件、レビュアー）に
途中から入り、ctx をコード理解の入口として使う開発者。ctx の既存ユーザー層と一致（ローカルでリポジトリを開いて読む道具）。
「全ユーザー向け」ではなく、**コード未習熟者・レビュアー**に絞る。

---

## 1. 「ファイル相関図」の解釈 — 3 案

「相関図」という言葉は曖昧なので、意味・ユースケース・元データを 3 通りに分けて提示する。

### (A) Co-change ネットワーク 【本命・MVP 対象】

- **意味**: 同一コミットで一緒に変更された 2 ファイルの間にエッジを引く。エッジ重み = 共変更回数（co-change count）。
- **ユースケース**: 暗黙の結合（API とそのテスト、`*.rs` とそのスナップショット、設定とローダ）を炙り出す。
  密なクラスタ = モジュール境界の発見。孤立ノード = 独立して動かせる安全なファイル。
- **元データ**: 全コミット（または直近 N 件）に対する `commit_files` の結果を総当たり集計。
  各コミットの変更ファイル集合 `S` について、`S` 内の全ペア `(a, b)` のカウンタを +1。
- **既存資産との接地**: `commit_files()` がまさにこの素データ。ノードの大きさには既存の
  `file_churn()`（commit 回数・最終変更時刻）をそのまま流用できる。

### (B) ディレクトリ／モジュール単位の集約相関 【拡張・スケール対策】

- **意味**: (A) のファイルノードを、指定深さ（例: 第 1〜2 階層）のディレクトリに畳んだもの。
  `crates/ctx-git/**` と `web/src/components/**` がどれだけ一緒に動くか、を粗い粒度で見せる。
- **ユースケース**: 大規模リポでファイルノードが数千に爆発する場合の既定ビュー。アーキテクチャ単位の結合確認。
- **元データ**: (A) と同じ集計結果を、ペアのパスを `path.split('/').slice(0, depth).join('/')` で畳んで再集計（フロント側で可能）。
- **接地**: 追加データ源不要。(A) のレスポンスをクライアントで再 group-by するだけ。

### (C) 特定ファイル中心の放射状ビュー（ego network） 【拡張】

- **意味**: 1 つのファイルを中心に置き、それと共変更されたファイルだけを重み順に放射状に並べる。
- **ユースケース**: 「いま開いている `lib.rs` を直したら、何を巻き込むか」をピンポイントで知りたいとき。
  file 詳細ビュー（既存の file-log 導線）から自然に繋がる。
- **元データ**: (A) の隣接リストから中心ノードの 1-hop 近傍を抽出（フロントでフィルタ）。
- **接地**: (A) のデータ + 既存の file 詳細／`file_log` 導線に直結。新規集計不要。

> **採用方針**: バックエンドは **(A) の co-change グラフを 1 本の API で返す**ことに集中し、
> (B)（畳み込み）と (C)（ego 抽出）は**そのレスポンスからフロントで派生させる**。
> こうすればサーバ側の新規実装は 1 関数 + 1 エンドポイントに収まり、3 つの見せ方をすべて賄える。

---

## 2. データ導出方法と新規 API スキーマ案

### 2.1 既存関数からの導出ロジック（co-change 集計）

現状、`commit_files(repo_root, hash)` は 1 コミット分しか返さず、全コミットを回すと
N 回のサブプロセス起動になり大規模リポで致命的に遅い。よって **新規バックエンド関数が必要**。

- **【新規】 `ctx_git::co_change_graph(repo_root, limit, since: Option<&str>) -> Result<CoChangeGraph>`**
  （`crates/ctx-git/src/lib.rs` に追加）
  - 実装は単一の `git log` 呼び出しで全コミットの変更ファイルをストリーム取得する。
    既存 `file_churn()` が使っている `git log --no-renames --name-only --format=%x00%ct` と
    **同じ 1-pass パターン**を流用し、コミット境界（`%x00`）ごとにファイル集合 `S` を貯め、
    `S` の全ペアを `HashMap<(u32,u32), u32>`（ファイルを ID 化したペア→共変更回数）に加算する。
    → サブプロセスは **1 回だけ**。`commit_files` の N 回起動は踏襲しない。
  - ノイズ除去のため、サーバ側で以下を適用:
    - `S.len() > MAX_FILES_PER_COMMIT`（例: 50）のコミットは除外（一括 reformat / vendoring を相関とみなさない）。
    - `binary` ファイルは除外（`file_churn` 同様 `--no-renames` で安定したパス）。
  - ノードの commits / last_commit_time は既存 `file_churn()` の戻り値をそのまま同梱（再利用）。

```rust
// 新規・スキーマ案のみ（実装しない）
pub struct CoChangeNode {
    pub path: String,
    pub commits: u32,          // file_churn 由来
    pub last_commit_time: i64, // file_churn 由来
}
pub struct CoChangeEdge {
    pub source: usize, // nodes インデックス
    pub target: usize,
    pub weight: u32,   // 共変更回数
}
pub struct CoChangeGraph {
    pub nodes: Vec<CoChangeNode>,
    pub edges: Vec<CoChangeEdge>,
    pub commits_scanned: u32,
    pub truncated: bool, // limit で打ち切ったか
}
```

### 2.2 新規 API エンドポイント案（既存 `/api/git/...` 規約準拠）

- **`GET /api/git/co-change`**
  - ハンドラ: `handle_co_change`（`crates/ctx-web/src/handlers/git.rs`）。
    既存ハンドラと同じく `crate::blocking::run(...)` でブロッキング実行し、`git_root_only(&state.root)` で repo root 解決。
  - Query params（既存 `RepoLogParams` と同じ命名作法）:
    - `limit`: 走査コミット数上限。既存同様 `clamp(1, 200)`… ではなく **相関は広い窓が要る**ので
      別定数で `clamp(1, 2000)` 程度を提案（既定 500）。`#TODO(agent): limit 上限値はパフォーマンス計測で確定`
    - `since`: git approxidate（例 `"180d"`）。`file_churn` がすでに受ける形式と一致。
    - `min_weight`: エッジ足切り（既定 2）。1 回しか共変更されていない偶発ペアを落とす。サーバ側適用でペイロード削減。
  - レスポンス JSON（`serde` 命名は既存ハンドラの snake_case 踏襲、ゼロ値は `skip_serializing_if`）:

```jsonc
{
  "nodes": [
    { "path": "crates/ctx-git/src/lib.rs", "commits": 31, "last_commit_time": 1718000000 },
    { "path": "crates/ctx-web/src/handlers/git.rs", "commits": 18, "last_commit_time": 1718000000 }
  ],
  "edges": [
    { "source": 0, "target": 1, "weight": 9 }
  ],
  "commits_scanned": 500,
  "truncated": false
}
```

  - **キャッシュ**: co-change は HEAD が動かない限り不変。既存 `DiffCache` と同じ発想で
    `head_oid + (limit, since, min_weight)` をフィンガープリントに `Arc<Vec<u8>>` をメモ化する
    `CoChangeCache` を `AppState` に追加することを推奨（MEMORY: web hotpath はキャッシュで最適化が定石）。
    rayon 並列化は web handler 内では無効果という既知知見があるため、**並列化ではなくキャッシュで対処**。

- `web/src/lib/api.ts` に `fetchCoChange(limit?, since?, minWeight?)` と型 `CoChangeResponse` を追加（既存 `fetchGitLog` と同型）。

---

## 3. 描画方式の選択肢と推奨

| 方式 | 既存性 | バンドル影響 | 大規模耐性 | 自前制御 | 評価 |
|---|---|---|---|---|---|
| **自前 SVG + 簡易力学なし固定レイアウト** | ◎ `git-graph.ts` の前例あり | ＋0 | △ レイアウト品質に限界 | ◎ | 力学が無いと相関図として読みにくい |
| **自前 SVG + 軽量 force シミュレーション（自作 or `d3-force` 追加）** | ○ SVG 描画の前例は流用可 | `d3-force` 単体は小（~20KB gz）。フルの d3 は不要 | ◎ 間引き併用で可 | ◎ | **本命** |
| **mermaid（既にバンドル済み `^11.15.0`）** | ○ 依存追加ゼロ | ＋0 | ✕ ノード数十でレイアウト破綻・再計算が重い | ✕ ホバー/クリック相関の細かい制御不可 | グラフ図には不向き |

### 推奨: **自前 SVG レンダラ + force-directed レイアウト（`d3-force` のみ追加）**

理由:
1. ctx には **手書き SVG グラフの実績**（`GitLogList.svelte` のインライン SVG + `git-graph.ts`）があり、
   `--ctx-*` テーマ変数・palette・dot/edge 描画パターンをそのまま相関図に転用できる。レンダリングは新規依存ゼロ。
2. レイアウト計算だけ `d3-force`（force-directed の事実上の標準、ツリーシェイク可で小さい）を**新規依存として追加**。
   mermaid は flowchart 文法レイアウト向けで、ノード数が増えるとレイアウトが破綻し、ホバー連動などの
   インタラクションを実装できないため、相関ネットワークには不適。**「mermaid 済みだから流用」は誤った節約**。
3. d3-force はシミュレーションを **数百 tick 走らせて座標を確定**し、以後は静的 SVG として描く運用にすれば
   Svelte の reactivity と干渉しない（tokio 関係なし・純フロント）。

> Ask First 対象（新規外部依存）: `d3-force` の追加は `package.json` への依存追加にあたる。
> mermaid 流用で妥協する選択肢も残すが、**相関図の体験品質を取るなら d3-force 追加を推奨**。導入可否は要承認。
> 代替（依存ゼロ厳守の場合）: 自前で簡易な Fruchterman-Reingold を ~60 行で実装も可能（`#TODO(agent)` 候補）。

### ノード数爆発への間引き／閾値戦略

co-change グラフはノード = 変更されたユニークファイル数で、大規模リポでは数千に達する。多段で間引く:

1. **サーバ側 `min_weight`（既定 2）**: 偶発的 1 回共変更を落とす（ペイロード最大の削減効果）。
2. **サーバ側 `since` / `limit`**: 走査窓を絞る（既定 500 commits）。
3. **クライアント Top-K**: 表示ノードを churn（commits 数）上位 K（例 60）に制限し、残りは「+N more」で畳む。
4. **(B) ディレクトリ畳み込みへ自動フォールバック**: ノードが閾値（例 120）超ならディレクトリ粒度を既定表示にする。
5. **(C) ego モード**: 特定ファイル選択時はその 1-hop 近傍のみ描画（常に小さく保たれる）。

---

## 4. UI フロー／配置案

### 配置: 既存 git log ビュー内の **新タブ（セグメント切替）**

現状 `App.svelte` の `route.name === 'gitlog'` は「左=`GitLogList`（コミット一覧）／右=`GitCommitDetail`」の 2 ペイン。
ここに以下を追加:

- 左ペイン上部（`GitLogList` の `<header>` 付近、`ref-select` の隣）に **ビュー切替セグメント** を置く:
  `[ Commits | Relations ]`。
- `Relations` 選択時、**右ペイン（または全幅）に新コンポーネント `GitCoChangeGraph.svelte` を表示**。
  既存 `ref-select`（branch/worktree）と `since` セレクタ（`30d / 90d / 1y / all`）を共有ヘッダに置く。
- ルーティング: `#/gitlog` 配下に `#/gitlog/relations`（任意で `?center=<path>` で (C) ego モード）を追加。
  既存 `router.svelte.ts` の `gitlog` パース分岐（`gitlog/` prefix）に 1 ケース足すだけで収まる。

### インタラクション

| 操作 | 挙動 |
|---|---|
| ノード hover | 当該ファイルと**直結エッジのみ**をハイライト、他を減光。ツールチップに path / commits / 最終変更 |
| ノード click | (C) ego モードに切替（その近傍だけ再描画） |
| ノード dblclick | 既存 **file 詳細**（`toFileHash(path)`）へ遷移し、そのまま `file-log` を見られる |
| エッジ hover | 共変更回数 weight と、寄与した代表コミットを表示（`#TODO(agent): 代表コミット表示は将来拡張`） |
| 凡例 | ノード色 = ディレクトリ（top-level dir ごとに palette）、サイズ = commits 数、線幅 = weight |

ノードを既存 file ビュー／file-log に橋渡しすることで、相関図は「発見 → 詳細閲覧」の入口として
既存導線に閉じる（孤立した新画面にしない）。

---

## 5. モックアップ（ASCII）

co-change ネットワーク（(A)）の見た目イメージ。線の太さ = weight、丸の大きさ = commits 数。

```
                 Git Log   [ Commits | ●Relations ]   ref:[HEAD ▾]  since:[90d ▾]
   ┌─────────────────────────────────────────────────────────────────────────┐
   │                                                                           │
   │     (ctx-git/src/lib.rs)══════════9═══════════(handlers/git.rs)           │
   │            ║                                        │                      │
   │            5                                        3                      │
   │            ║                                        │                      │
   │     (git-graph.ts)──2──(GitLogList.svelte)     (api.ts)                    │
   │                                  │                                         │
   │                                  4                                         │
   │                                  │                                         │
   │                          (router.svelte.ts)        ·(README.md)  ← 孤立    │
   │                                                                           │
   │   ● size = commits(churn)   ══ = 高 weight   ── = 低 weight   color = dir  │
   │   [ Showing top 60 of 248 files · +188 more ]   [▢ Group by directory]     │
   └─────────────────────────────────────────────────────────────────────────┘
```

ego モード（(C)・`lib.rs` を中心に選択）:

```
        (commit-files 経由の隣接のみ)
                handlers/git.rs
                      │ 9
        git-graph.ts  │
            2 \       │
               \      │
   tests/* ─4─ ● crates/ctx-git/src/lib.rs ─5─ Cargo.toml
                      │
                      │ 3
                  api.ts
   ← click で中心ファイル切替 / dblclick で file 詳細へ
```

---

## 6. MVP スコープと拡張・スケール注意点

### MVP（最小実装スコープ）

1. **backend**: `ctx_git::co_change_graph()`（1-pass `git log`、`min_weight`/`limit`/`since`）+ `CoChangeCache`。
2. **API**: `GET /api/git/co-change` ハンドラ + `api.ts` の `fetchCoChange`。
3. **frontend**: `GitCoChangeGraph.svelte`（自前 SVG 描画 + force レイアウト）、左ヘッダのビュー切替セグメント、
   Top-K 間引き、hover ハイライト、dblclick で file 詳細遷移。
4. **解釈は (A) co-change ネットワークのみ**。レイアウトは d3-force（承認後）or 自前簡易力学。

MVP 完了の体験: 「Relations タブを開く → 主要ファイルのクラスタが見える → ノードを叩くと file 詳細へ飛べる」。

### 拡張余地（MVP 後）

- (B) ディレクトリ畳み込みトグル（同一レスポンスをフロント再集計）。
- (C) ego モードの URL 共有（`?center=`）。
- エッジ→寄与コミット一覧（`commit_files` で hover 時に lazy 取得）。
- author 軸の相関（誰と誰が同じファイルを触るか）— `repo_log` の author を流用。
- `file_churn` を活かした recency 着色（最近動いたクラスタを強調）。

### パフォーマンス／スケール注意点

- **致命傷の回避**: 「全コミットに `commit_files` を N 回呼ぶ」は禁止（サブプロセス N 起動）。
  必ず `file_churn` と同じ **単一 `git log` ストリーム集計**にする。
- co-change のペア数は最悪 O(Σ |S_i|²)。巨大コミット（mass reformat・vendoring）が二乗を爆発させるため、
  `MAX_FILES_PER_COMMIT` で大型コミットを集計から除外（既知の dist 再ベンダリングのような一括変更を弾く）。
- レスポンスサイズはサーバ側 `min_weight` 足切りで抑える（クライアント Top-K の前段）。
- キャッシュは HEAD oid フィンガープリントで無効化（既存 `DiffCache` 同型）。`rayon` 並列化は web では効かない既知知見に従い使わない。
- force シミュレーションはクライアント。数百ノードを超えたら (B) ディレクトリ畳み込みへ自動フォールバックして tick 数を抑える。

---

## 重複チェック

- 既存に co-change / ファイル相関を描く機能は **存在しない**（`GitLogList` は時間軸 commit graph のみ）。
- `file_churn()` はライブラリに実装済みだが **API 未公開・フロント未使用**。本提案はこれを初めて実利用する（重複ではなく活用）。
- commit graph（lane 描画）とは描く軸（時間 vs 構造）が異なり機能重複しない。

---

## Alternative Framings Considered

1. **「コミット履歴が時系列でしか追えない」** → 時間軸は既に commit graph が解決済み。相関（構造軸）こそ空白なので不採用。
2. **「ファイルごとの変更頻度（ホットスポット）が見えない」（churn 単体ヒートマップ）** → `file_churn` 単体可視化は
   *関係性*ではなく*個別量*。ユーザー要望の「相関図」を満たさないため主軸から外す（ノードサイズとして従属利用に留める）。
3. **「コードの依存関係（import グラフ）が見えない」** → これは静的解析（tree-sitter）由来の別物で、git log とは無関係。
   要望は git log 部分なので、履歴由来の co-change に絞る（静的依存は別 RFC 領域）。

---

## 優先度・スコアリング

- **Horizon**: `H2`（隣接する新規ケイパビリティ。既存 git データ基盤を土台に、新しい解析・可視化軸を足す）。
  `H1`（churn ヒートマップだけ出す）も検討したが、ユーザーが求めたのは*相関*であり、単一ファイル量の可視化では
  「一緒に変わる」というジョブを満たせず bold option が負ける理由が薄い → より価値の高い H2 を採用。
- **Impact–Effort**: **Big Bet**（体験インパクト大／backend 集計 + force 描画で中規模工数）。
- **RICE**:
  - Reach: ctx で git log を開く層のうちコード理解目的のセッション。控えめに四半期 **40%**（セグメント限定、全ユーザーではない）。
  - Impact: **2（High 寄り）** — 波及把握という未充足ジョブに直接効くが、Impact=3 は乱発しない方針に従い 2。
  - Confidence: **50%** — co-change が結合を表すのは研究知見で裏付けがあるが、ctx ユーザーでの実需は未検証ゆえ既定値。
  - Effort: **約 2.5（人月相当）** — backend 集計 + キャッシュ + 新 Svelte コンポーネント + force 導入 + テスト/ドキュメント + 30% バッファ込み。
  - **RICE = (40 × 2 × 0.5) / 2.5 = 16 → Low 帯**。
    ただし Confidence が新規性ゆえ構造的に低いだけで、H2 の bold bet として横並び比較ではなく Horizon 内で評価する。

---

## テスト可能な仮説と Fail Condition

- **Hypothesis**: Newcomer ペルソナに対し、Relations ビューを提供すると「変更前に波及範囲を確認できた」と感じる割合が上がる。
  - 指標: Relations タブを開いたセッションのうち、ノードから file 詳細／file-log へ遷移した割合。
  - Baseline: 0%（機能が存在しない）。Target: 導入後の Relations セッションで **≥ 30%** が file 詳細へ遷移。
  - 検証方法: フロントの遷移計測（既存 router 計装に乗せる）＋ 5 名規模のコード探索タスク観察（think-aloud）。
- **Fail Condition（kill 基準）**: 公開後 30 日で **Relations タブを開いたセッションが git log セッション全体の 2% 未満**、
  かつ観察で「読み取れない／重くて使えない」が支配的 → ビューを撤去（commit graph に一本化）。

---

## 受け入れ条件（Acceptance Criteria）

- [ ] `GET /api/git/co-change` が `nodes/edges/commits_scanned/truncated` を返し、`min_weight`/`since`/`limit` が効く。
- [ ] 集計は単一 `git log` 呼び出し（`commit_files` の N 回呼び出しをしない）。`MAX_FILES_PER_COMMIT` 超のコミットは除外。
- [ ] レスポンスは HEAD oid + パラメータをキーにキャッシュされ、2 回目以降が再集計なしで返る。
- [ ] git log 左ヘッダに `[ Commits | Relations ]` 切替があり、`#/gitlog/relations` で deep link 可能。
- [ ] ノード hover で直結エッジがハイライト、dblclick で既存 file 詳細へ遷移する。
- [ ] ノード数が閾値超のとき (B) ディレクトリ畳み込みに自動フォールバックする。
- [ ] 無コミット／単一コミットのリポでも空状態を壊さず表示する。

---

## Validation Strategy / Next Handoff

- **Validation**: まず Fake Door 不要（描画体験が価値の中心のため）。`Forge` で d3-force レイアウトの
  プロトタイプ（ダミー JSON）を先に作り、大規模リポのノード数でレイアウトが破綻しないかを描画前に検証することを推奨。
- **Next**: `Forge`（force レイアウト + 間引きの描画プロトタイプ検証）→ 問題なければ `Builder`（backend `co_change_graph` + API + キャッシュ）/ `Artisan`（`GitCoChangeGraph.svelte`）。
  外部依存 `d3-force` 追加の可否はユーザー承認待ち（Ask First）。
```
