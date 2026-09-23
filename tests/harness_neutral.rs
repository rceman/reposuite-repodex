//! Architecture guard (addendum §14): production `src/**` must carry zero
//! harness-specific semantic dependencies. Scans every `.rs` file under `src/`
//! for runtime/harness identifiers and requires none.
//!
//! `acp` is matched on word boundaries only to avoid substring false-positives.

use std::fs;
use std::path::Path;

/// Whole-word identifier match (case-insensitive).
fn has_word(text: &str, needle: &str) -> bool {
    let t = text.to_lowercase();
    let n = needle.to_lowercase();
    let mut start = 0;
    while let Some(i) = t[start..].find(&n) {
        let i = start + i;
        let before =
            i == 0 || !t.as_bytes()[i - 1].is_ascii_alphanumeric() && t.as_bytes()[i - 1] != b'_';
        let end = i + n.len();
        let after = end >= t.len()
            || !t.as_bytes()[end].is_ascii_alphanumeric() && t.as_bytes()[end] != b'_';
        if before && after {
            return true;
        }
        start = i + 1;
    }
    false
}

fn walk(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, files);
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            files.push(p);
        }
    }
}

#[test]
fn no_harness_specific_production_dependencies() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);
    let forbidden = [
        "devin",
        "atif",
        "codex",
        "opencode",
        "claude",
        "app_server",
        "app-server",
        "acp",
    ];
    let mut violations = Vec::new();
    for f in &files {
        let text = fs::read_to_string(f).unwrap_or_default();
        for id in &forbidden {
            if has_word(&text, id) {
                violations.push(format!("{}: `{id}`", f.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "HARNESS_SPECIFIC_PRODUCTION_DEPENDENCIES = {}:\n{}",
        violations.len(),
        violations.join("\n")
    );
}
