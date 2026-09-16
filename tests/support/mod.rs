//! Shared test helpers.
//!
//! `REPODEX_BLESS=1 cargo test --test expected_facts` rewrites the expected
//! canonical fact files. Blessing is a deliberate review action: a changed
//! expectation must be read, not accepted.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use repodex::{Analyzer, AnalyzerConfig, Declaration, FileAnalysis, LanguageId, ReferenceKind};

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixtures_root() -> PathBuf {
    repo_root().join("fixtures")
}

pub fn analyzer() -> Analyzer {
    Analyzer::new(AnalyzerConfig::default()).expect("default analyzer configuration must be valid")
}

pub fn fixture(relative: &str) -> PathBuf {
    fixtures_root().join(relative)
}

pub fn analyze_fixture(relative: &str) -> FileAnalysis {
    let root = fixtures_root();
    analyzer().analyze_path(&root, &fixture(relative))
}

pub fn analyze_source(language: LanguageId, relative_path: &str, source: &[u8]) -> FileAnalysis {
    analyzer().analyze_bytes(relative_path, language, source)
}

/// Every committed fixture file, sorted deterministically.
pub fn all_fixture_files() -> Vec<String> {
    let mut found = Vec::new();
    collect(&fixtures_root(), &fixtures_root(), &mut found);
    found.sort();
    found
}

fn collect(root: &Path, directory: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut paths = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some("expected") {
                continue;
            }
            collect(root, &path, found);
        } else if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
            if matches!(extension, "rs" | "go" | "py" | "php") {
                let relative = path
                    .strip_prefix(root)
                    .expect("fixture inside fixtures root")
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push(relative);
            }
        }
    }
}

pub fn expected_path(relative: &str) -> PathBuf {
    fixtures_root()
        .join("expected")
        .join(format!("{relative}.txt"))
}

/// Compare the complete canonical fact set of a fixture against its committed
/// expectation.
pub fn assert_expected_facts(relative: &str) {
    let analysis = analyze_fixture(relative);
    let actual = analysis.canonical_text();
    let path = expected_path(relative);
    if std::env::var_os("REPODEX_BLESS").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create expected directory");
        }
        std::fs::write(&path, &actual).expect("write expected facts");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing expectation {}: {error}\nrun `REPODEX_BLESS=1 cargo test --test expected_facts` after reviewing",
            path.display()
        )
    });
    if expected != actual {
        panic!(
            "canonical facts for `{relative}` differ from {}:\n{}",
            path.display(),
            render_diff(&expected, &actual)
        );
    }
}

fn render_diff(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let mut out = String::new();
    let mut differences = 0usize;
    for index in 0..expected_lines.len().max(actual_lines.len()) {
        let left = expected_lines.get(index).copied();
        let right = actual_lines.get(index).copied();
        if left == right {
            continue;
        }
        differences += 1;
        out.push_str(&format!("  line {index}:\n"));
        out.push_str(&format!("    expected: {}\n", left.unwrap_or("<none>")));
        out.push_str(&format!("    actual:   {}\n", right.unwrap_or("<none>")));
        if differences >= 20 {
            out.push_str("  ... (truncated)\n");
            break;
        }
    }
    out
}

/// `kind name scope_path flags` for every declaration, in canonical order.
pub fn declaration_summary(analysis: &FileAnalysis) -> Vec<String> {
    analysis
        .declarations
        .iter()
        .map(|declaration| {
            let flags = declaration
                .flags
                .iter()
                .map(|flag| flag.as_str())
                .collect::<Vec<_>>()
                .join("|");
            let mut parts = vec![
                declaration.kind.as_str().to_string(),
                declaration.name.clone(),
                analysis.scope_path(declaration.scope_id),
            ];
            if !flags.is_empty() {
                parts.push(flags);
            }
            parts.join(" ")
        })
        .collect()
}

/// `form callee dyn nullsafe` for every call-like occurrence.
pub fn call_summary(analysis: &FileAnalysis) -> Vec<String> {
    analysis
        .calls
        .iter()
        .map(|call| {
            format!(
                "{} {} dyn={} nullsafe={}",
                call.form.as_str(),
                call.callee_written,
                call.dynamic_callee,
                call.nullsafe
            )
        })
        .collect()
}

/// `kind written` for every unresolved reference.
pub fn reference_summary(analysis: &FileAnalysis) -> Vec<String> {
    analysis
        .references
        .iter()
        .map(|reference| format!("{} {}", reference.kind.as_str(), reference.written))
        .collect()
}

/// `kind name scope_path` for every scope.
pub fn scope_summary(analysis: &FileAnalysis) -> Vec<String> {
    analysis
        .scopes
        .iter()
        .map(|scope| {
            format!(
                "{} {} {}",
                scope.kind.as_str(),
                scope.name.as_deref().unwrap_or("-"),
                scope
                    .parent_scope_id
                    .map(|id| analysis.scope_path(id))
                    .unwrap_or_else(|| "-".to_string())
            )
        })
        .collect()
}

/// Test evidence attached to a declaration, as `kind:detail`.
pub fn test_evidence(analysis: &FileAnalysis, name: &str) -> Vec<String> {
    analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.name == name)
        .flat_map(|declaration| {
            declaration
                .test_evidence
                .iter()
                .map(|evidence| format!("{}:{}", evidence.kind.as_str(), evidence.detail))
        })
        .collect()
}

/// A source-grounded identity key for one declaration.
///
/// Go declares receiver methods at file scope, so `name` plus lexical scope is
/// not unique for Go. The receiver type written in the declaration is part of
/// the identity, exactly as the model requires it to be recorded separately.
pub fn declaration_identity(analysis: &FileAnalysis, declaration: &Declaration) -> String {
    let receiver = analysis
        .references
        .iter()
        .find(|reference| {
            reference.declaration_id == Some(declaration.declaration_id)
                && reference.kind == ReferenceKind::ReceiverType
        })
        .map(|reference| reference.written.clone());
    let base = format!(
        "{} {}",
        declaration.name,
        analysis.scope_path(declaration.scope_id)
    );
    match receiver {
        Some(receiver) => format!("{base} receiver={receiver}"),
        None => base,
    }
}

/// Byte offsets of a declaration's full range.
pub fn declaration_range(analysis: &FileAnalysis, name: &str) -> Option<(u32, u32)> {
    analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == name)
        .map(|declaration| (declaration.range.byte_start, declaration.range.byte_end))
}

/// Slice a fixture's source bytes.
pub fn fixture_source(relative: &str) -> Vec<u8> {
    std::fs::read(fixture(relative)).expect("fixture readable")
}

pub fn slice(relative: &str, start: u32, end: u32) -> String {
    let bytes = fixture_source(relative);
    String::from_utf8_lossy(&bytes[start as usize..end as usize]).to_string()
}

static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A scratch directory under the platform temp directory.
///
/// Tests must never write into the user's real home directory.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "repodex-test-{}-{}-{}",
            std::process::id(),
            label,
            counter
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&self, relative: &str, contents: &[u8]) -> PathBuf {
        let target = self.path.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(&target, contents).expect("write file");
        target
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
