use std::path::Path;

use super::*;
use serde::Serialize;

/// One budget item (mirrors `budget.Item`).
#[derive(Clone)]
pub(crate) struct BudgetItem {
    pub(crate) path: String,
    pub(crate) tokens: i64,
    pub(crate) reason: String,
    pub(crate) group: String,
}

/// Budget plan result (mirrors `budget.Result`).
pub(crate) struct BudgetResult {
    pub(crate) budget: i64,
    pub(crate) used: i64,
    pub(crate) included: Vec<BudgetItem>,
    pub(crate) excluded: Vec<BudgetItem>,
}

/// A flattened file entry (path/tokens/role) used by the budget planner.
pub(crate) struct BudgetFile {
    pub(crate) path: String,
    pub(crate) tokens: i64,
    pub(crate) role: String,
}

/// Render the `--budget` view. Walks the tree, flattens it to files, runs the
/// greedy plan, then emits JSON (`JSONBudget`) or the text view
/// (`BudgetWithOptions{Plain}`) — matching Go's dispatch inside the budget path.
pub(crate) fn render_root_budget(
    root: &Path,
    tree_opts: &TreeBuildOpts,
    budget: i64,
    want_json: bool,
    plain: bool,
) -> Result<(), String> {
    let node = build_root_tree(root, tree_opts)?.ok_or_else(|| "root is ignored".to_string())?;
    let mut files = Vec::new();
    flatten_budget_files(&node, &mut files);
    let result = budget_plan(&files, budget);
    if want_json {
        render_json_budget(&result);
    } else {
        render_budget_text(&result, plain);
    }
    Ok(())
}

/// Flatten the enriched tree into the file list consumed by `budget_plan`,
/// skipping directories and the root node (mirrors `walk.Flatten` + Go's
/// `Plan` guard `file.IsDir || file.Path == "."`). Paths are already
/// slash-separated by the tree builder.
pub(crate) fn flatten_budget_files(node: &JsonTreeNode, out: &mut Vec<BudgetFile>) {
    if !node.is_dir && node.path != "." {
        out.push(BudgetFile {
            path: node.path.clone(),
            tokens: node.metadata.tokens,
            role: node.metadata.role.clone(),
        });
    }
    for child in &node.children {
        flatten_budget_files(child, out);
    }
}

/// Mirrors `internal/budget/budget.go:Plan`. Greedy selection by
/// (role priority, tokens, path); excludes binary/generated/too-large files and
/// any candidate that overflows the budget.
pub(crate) fn budget_plan(files: &[BudgetFile], token_budget: i64) -> BudgetResult {
    let mut result = BudgetResult {
        budget: token_budget,
        used: 0,
        included: Vec::new(),
        excluded: Vec::new(),
    };
    if token_budget <= 0 {
        return result;
    }

    let mut candidates: Vec<(i64, BudgetItem)> = Vec::new();
    for file in files {
        let tokens = file.tokens;
        let role = file.role.as_str();
        let path = file.path.clone();
        if tokens == 0 {
            result.excluded.push(BudgetItem {
                path,
                tokens,
                reason: "binary".to_string(),
                group: String::new(),
            });
        } else if role == "generated" {
            result.excluded.push(BudgetItem {
                path,
                tokens,
                reason: "generated".to_string(),
                group: String::new(),
            });
        } else if tokens > token_budget / 2 {
            result.excluded.push(BudgetItem {
                path,
                tokens,
                reason: "too large".to_string(),
                group: String::new(),
            });
        } else {
            candidates.push((
                budget_role_priority(role),
                BudgetItem {
                    path,
                    tokens,
                    reason: String::new(),
                    group: String::new(),
                },
            ));
        }
    }

    // sort.SliceStable: (priority ASC, tokens ASC, path ASC).
    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.tokens.cmp(&b.1.tokens))
            .then(a.1.path.cmp(&b.1.path))
    });

    for (_, mut item) in candidates {
        if result.used + item.tokens > token_budget {
            item.reason = "budget exceeded".to_string();
            result.excluded.push(item);
            continue;
        }
        item.reason = String::new();
        result.used += item.tokens;
        result.included.push(item);
    }

    budget_assign_groups(&mut result.included);
    budget_assign_groups(&mut result.excluded);
    result
}

/// Mirrors `budget.rolePriority`.
pub(crate) fn budget_role_priority(role: &str) -> i64 {
    match role {
        "entry" | "core" | "route" | "config" => 0,
        "test" | "util" => 1,
        "doc" | "unknown" | "" => 2,
        _ => 1,
    }
}

/// Mirrors `budget.assignGroups`: a file gets group = its parent dir when 3+
/// files in the same list share that dir.
pub(crate) fn budget_assign_groups(items: &mut [BudgetItem]) {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for item in items.iter() {
        let dir = budget_parent_dir(&item.path);
        if dir == "." {
            continue;
        }
        *counts.entry(dir).or_insert(0) += 1;
    }
    for item in items.iter_mut() {
        let dir = budget_parent_dir(&item.path);
        if dir == "." {
            continue;
        }
        if counts.get(&dir).copied().unwrap_or(0) >= 3 {
            item.group = dir;
        }
    }
}

/// Mirrors `filepath.ToSlash(filepath.Dir(path))`.
pub(crate) fn budget_parent_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(idx) => path[..idx].to_string(),
        None => ".".to_string(),
    }
}

/// Mirrors `render.JSONBudget` + `toJSONBudgetItems`. Field order matches the
/// Go struct tags: budget, used, included, excluded; per item: path, tokens,
/// group (omitempty), reason. 2-space indent + trailing newline.
pub(crate) fn render_json_budget(result: &BudgetResult) {
    #[derive(Serialize)]
    struct JsonBudgetItem {
        path: String,
        tokens: i64,
        #[serde(skip_serializing_if = "String::is_empty")]
        group: String,
        reason: String,
    }
    #[derive(Serialize)]
    struct JsonBudget {
        budget: i64,
        used: i64,
        included: Vec<JsonBudgetItem>,
        excluded: Vec<JsonBudgetItem>,
    }
    let to_items = |items: &[BudgetItem]| -> Vec<JsonBudgetItem> {
        items
            .iter()
            .map(|i| JsonBudgetItem {
                path: i.path.clone(),
                tokens: i.tokens,
                group: i.group.clone(),
                reason: i.reason.clone(),
            })
            .collect()
    };
    let doc = JsonBudget {
        budget: result.budget,
        used: result.used,
        included: to_items(&result.included),
        excluded: to_items(&result.excluded),
    };
    let mut out = serde_json::to_string_pretty(&doc).unwrap_or_default();
    out.push('\n');
    print!("{out}");
}

pub(crate) const BUDGET_BAR_WIDTH: i64 = 20;

/// Mirrors `render.BudgetWithOptions`.
pub(crate) fn render_budget_text(result: &BudgetResult, plain: bool) {
    let mut out = String::new();
    if plain {
        render_plain_budget(&mut out, result);
        print!("{out}");
        return;
    }
    out.push_str(&format!(
        "Context Budget: {} tokens\n\n",
        format_grouped_number(result.budget)
    ));
    out.push_str(&format!(
        "[{}] {} / {} tokens\n\n",
        budget_bar(result.used, result.budget),
        format_grouped_number(result.used),
        format_grouped_number(result.budget)
    ));

    let included = aggregate_budget_items(&result.included);
    let excluded = aggregate_budget_items(&result.excluded);

    out.push_str(&format!("Included ({} files)\n", result.included.len()));
    render_budget_items(&mut out, &included, true);
    out.push('\n');

    out.push_str(&format!("Excluded ({} files)\n", result.excluded.len()));
    render_budget_items(&mut out, &excluded, false);

    print!("{out}");
}

/// Mirrors `render.renderPlainBudget`.
pub(crate) fn render_plain_budget(out: &mut String, result: &BudgetResult) {
    let percent = if result.budget > 0 {
        ((result.used as f64 / result.budget as f64) * 100.0 + 0.5).floor() as i64
    } else {
        0
    };
    out.push_str(&format!(
        "Context Budget: {} tokens\n",
        format_grouped_number(result.budget)
    ));
    out.push_str(&format!(
        "Used: {} / {} ({}%)\n\n",
        format_grouped_number(result.used),
        format_grouped_number(result.budget),
        percent
    ));
    for item in &result.included {
        out.push_str(&format!(
            "+ {}  ({} tokens)\n",
            item.path,
            format_grouped_number(item.tokens)
        ));
    }
    for item in &result.excluded {
        out.push_str(&format!("- {}  ({})\n", item.path, item.reason));
    }
}

/// Mirrors `render.budgetBar`.
pub(crate) fn budget_bar(used: i64, token_budget: i64) -> String {
    if token_budget <= 0 {
        return "\u{2591}".repeat(BUDGET_BAR_WIDTH as usize);
    }
    let mut filled =
        ((used as f64 / token_budget as f64) * BUDGET_BAR_WIDTH as f64 + 0.5).floor() as i64;
    if filled < 0 {
        filled = 0;
    }
    if filled > BUDGET_BAR_WIDTH {
        filled = BUDGET_BAR_WIDTH;
    }
    format!(
        "{}{}",
        "\u{2588}".repeat(filled as usize),
        "\u{2591}".repeat((BUDGET_BAR_WIDTH - filled) as usize)
    )
}

/// Mirrors `render.renderBudgetItems` (tree-connector list with padded paths).
pub(crate) fn render_budget_items(out: &mut String, items: &[BudgetItem], show_tokens: bool) {
    const CONNECTOR_MID: &str = "\u{251c}\u{2500} "; // "├─ "
    const CONNECTOR_LAST: &str = "\u{2514}\u{2500} "; // "└─ "
    if items.is_empty() {
        out.push_str("\u{2514}\u{2500} -\n");
        return;
    }
    let width = max_budget_path_width(items);
    for (i, item) in items.iter().enumerate() {
        let connector = if i == items.len() - 1 {
            CONNECTOR_LAST
        } else {
            CONNECTOR_MID
        };
        if show_tokens {
            out.push_str(&format!(
                "{}{:<width$}  {} tokens\n",
                connector,
                item.path,
                format_grouped_number(item.tokens),
                width = width
            ));
        } else {
            out.push_str(&format!(
                "{}{:<width$}  {}\n",
                connector,
                item.path,
                item.reason,
                width = width
            ));
        }
    }
}

/// Mirrors `render.maxBudgetPathWidth` (min 28).
pub(crate) fn max_budget_path_width(items: &[BudgetItem]) -> usize {
    let mut width = 0;
    for item in items {
        if item.path.len() > width {
            width = item.path.len();
        }
    }
    width.max(28)
}

/// Mirrors `render.aggregateBudgetItems`: collapse 3+ same-group items into a
/// single `dir/**` aggregate row, preserving first-appearance order.
pub(crate) fn aggregate_budget_items(items: &[BudgetItem]) -> Vec<BudgetItem> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for item in items {
        if !item.group.is_empty() {
            *counts.entry(item.group.clone()).or_insert(0) += 1;
            continue;
        }
        let dir = budget_parent_dir(&item.path);
        if dir != "." {
            *counts.entry(dir).or_insert(0) += 1;
        }
    }

    let mut grouped: std::collections::HashMap<String, BudgetItem> =
        std::collections::HashMap::new();
    let mut seen_group: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut output: Vec<BudgetItem> = Vec::with_capacity(items.len());
    for item in items {
        let mut group = item.group.clone();
        if group.is_empty() {
            let dir = budget_parent_dir(&item.path);
            if counts.get(&dir).copied().unwrap_or(0) >= 3 {
                group = dir;
            }
        }
        if group.is_empty() || counts.get(&group).copied().unwrap_or(0) < 3 {
            output.push(item.clone());
            continue;
        }
        let agg = grouped.entry(group.clone()).or_insert_with(|| BudgetItem {
            path: format!("{group}/**"),
            tokens: 0,
            reason: item.reason.clone(),
            group: String::new(),
        });
        if agg.reason != item.reason {
            agg.reason = "multiple reasons".to_string();
        }
        agg.tokens += item.tokens;
        if seen_group.insert(group.clone()) {
            output.push(BudgetItem {
                path: format!("{group}/**"),
                tokens: 0,
                reason: String::new(),
                group: String::new(),
            });
        }
    }

    for item in output.iter_mut() {
        if let Some(group) = item.path.strip_suffix("/**") {
            if let Some(agg) = grouped.get(group) {
                *item = agg.clone();
            }
        }
    }
    output
}
