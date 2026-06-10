// crates/ctx-relations/src/languages/php.rs
//
// Port of internal/relations/php.go.

use std::collections::HashSet;
use std::fs;

use crate::patterns;

/// Mirror of `phpPSR4Map`.
#[derive(Debug, Clone)]
pub struct PhpPSR4Map {
    pub prefix: String,
    pub dir: String,
}

/// Mirror of `phpPSR4`.
#[derive(Debug, Clone, Default)]
pub struct PhpPSR4 {
    pub mappings: Vec<PhpPSR4Map>,
}

/// Mirror of `readComposerPSR4(root)`.
pub fn read_composer_psr4(root: &str) -> Option<PhpPSR4> {
    let path = std::path::Path::new(root).join("composer.json");
    let data = fs::read(&path).ok()?;
    let parsed: serde_json::Value = serde_json::from_slice(&data).ok()?;
    let mut ms = Vec::new();
    for section in ["autoload", "autoload-dev"] {
        if let Some(psr4) = parsed
            .get(section)
            .and_then(|v| v.get("psr-4"))
            .and_then(|v| v.as_object())
        {
            for (prefix, value) in psr4 {
                if let Some(s) = value.as_str() {
                    ms.push(PhpPSR4Map {
                        prefix: prefix.clone(),
                        dir: clean_psr_dir(s),
                    });
                } else if let Some(arr) = value.as_array() {
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            ms.push(PhpPSR4Map {
                                prefix: prefix.clone(),
                                dir: clean_psr_dir(s),
                            });
                        }
                    }
                }
            }
        }
    }
    if ms.is_empty() {
        return None;
    }
    // Sort by descending prefix length (stable).
    ms.sort_by(|a, b| b.prefix.len().cmp(&a.prefix.len()));
    Some(PhpPSR4 { mappings: ms })
}

fn clean_psr_dir(d: &str) -> String {
    let d = d.replace('\\', "/");
    d.trim_end_matches('/').to_string()
}

/// Mirror of `resolvePHPImports`.
pub fn resolve_php_imports(
    abs_path: &std::path::Path,
    ps: Option<&PhpPSR4>,
    all: &HashSet<String>,
) -> Vec<String> {
    let ps = match ps {
        Some(p) if !p.mappings.is_empty() => p,
        _ => return Vec::new(),
    };
    let data = match fs::read_to_string(abs_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let src = strip_php_comments(&data);
    let re = patterns::php_use_re();
    let mut out = Vec::new();
    for caps in re.captures_iter(&src) {
        let body = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        for fqn in expand_php_group_use(body) {
            if let Some(resolved) = resolve_php_class(&fqn, ps, all) {
                out.push(resolved);
            }
        }
    }
    out
}

fn strip_php_comments(src: &str) -> String {
    let s = patterns::php_block_comment_re().replace_all(src, "");
    let s = patterns::php_line_comment_re().replace_all(&s, "");
    let s = patterns::php_hash_comment_re().replace_all(&s, "");
    s.into_owned()
}

/// Mirror of `expandPHPGroupUse`. Handles both single and group form.
fn expand_php_group_use(body: &str) -> Vec<String> {
    if let Some(i) = body.find('{') {
        if let Some(j) = body.find('}') {
            if j > i {
                let mut prefix = body[..i].trim().to_string();
                prefix = prefix.trim_end_matches('\\').to_string();
                let inner = &body[i + 1..j];
                let mut out = Vec::new();
                for part in inner.split(',') {
                    let name = strip_php_as(part.trim());
                    if name.is_empty() {
                        continue;
                    }
                    out.push(format!("{prefix}\\{name}"));
                }
                return out;
            }
        }
    }
    vec![strip_php_as(body).to_string()]
}

fn strip_php_as(s: &str) -> &str {
    let lower = s.to_ascii_lowercase();
    if let Some(i) = lower.find(" as ") {
        s[..i].trim()
    } else {
        s.trim()
    }
}

/// Mirror of `resolvePHPClass`.
fn resolve_php_class(fqn: &str, ps: &PhpPSR4, all: &HashSet<String>) -> Option<String> {
    let fqn = fqn.trim_start_matches('\\').to_string();
    if fqn.is_empty() {
        return None;
    }
    for m in &ps.mappings {
        if !fqn.starts_with(&m.prefix) {
            continue;
        }
        let rest = &fqn[m.prefix.len()..];
        let rel_path = rest.replace('\\', "/");
        if rel_path.is_empty() {
            continue;
        }
        let full = if m.dir.is_empty() {
            format!("{rel_path}.php")
        } else {
            format!("{}/{rel_path}.php", m.dir)
        };
        if all.contains(&full) {
            return Some(full);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_single_use() {
        assert_eq!(
            expand_php_group_use(r#"Foo\Bar"#),
            vec![r#"Foo\Bar"#.to_string()]
        );
    }

    #[test]
    fn expand_group_use() {
        let got = expand_php_group_use(r#"Foo\{A, B as B2}"#);
        assert_eq!(got, vec![r#"Foo\A"#.to_string(), r#"Foo\B"#.to_string()]);
    }
}
