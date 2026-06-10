use std::path::Path;

#[derive(Debug)]
pub(crate) struct PackIgnore {
    pub(crate) patterns: Vec<String>,
}

impl PackIgnore {
    pub(crate) fn load(root: &Path, extra: &[String], respect_ctxignore: bool) -> Self {
        let mut patterns = vec![
            "node_modules/**".to_string(),
            "dist/**".to_string(),
            "coverage/**".to_string(),
            "*.lock".to_string(),
            ".git/**".to_string(),
        ];
        patterns.extend(extra.iter().cloned());
        if respect_ctxignore {
            let ctxignore = root.join(".ctxignore");
            if let Ok(body) = std::fs::read_to_string(ctxignore) {
                for line in body.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                        continue;
                    }
                    patterns.push(trimmed.to_string());
                }
            }
        }
        Self { patterns }
    }

    pub(crate) fn is_ignored(&self, root: &Path, path: &Path, is_dir: bool) -> bool {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        self.is_ignored_rel(&rel, is_dir)
    }

    pub(crate) fn is_ignored_rel(&self, rel: &str, is_dir: bool) -> bool {
        self.patterns
            .iter()
            .any(|pattern| matches_ignore_pattern(pattern, rel, is_dir))
    }
}

pub(crate) fn matches_ignore_pattern(pattern: &str, rel: &str, is_dir: bool) -> bool {
    let pattern = pattern.trim().trim_start_matches("./");
    if pattern.is_empty() {
        return false;
    }
    if let Some(dir) = pattern.strip_suffix('/') {
        return rel == dir || rel.starts_with(&format!("{dir}/")) || (is_dir && rel.ends_with(dir));
    }
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return rel == prefix || rel.starts_with(&format!("{prefix}/"));
    }
    if !pattern.contains('*') {
        return rel == pattern || rel.ends_with(&format!("/{pattern}"));
    }
    glob_match(pattern, rel)
}

pub(crate) fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    if !parts[0].is_empty() {
        let Some(next) = rest.strip_prefix(parts[0]) else {
            return false;
        };
        rest = next;
    }
    for part in parts.iter().skip(1).take(parts.len().saturating_sub(2)) {
        if part.is_empty() {
            continue;
        }
        let Some(index) = rest.find(part) else {
            return false;
        };
        rest = &rest[index + part.len()..];
    }
    if let Some(last) = parts.last() {
        if !last.is_empty() {
            return rest.ends_with(last);
        }
    }
    true
}
