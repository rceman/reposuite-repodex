//! TASK 2 real-file incremental correctness.
//!
//! TASK 1 proved incremental equivalence on synthetic fixtures. This suite runs
//! the same contract against real production files from the TASK 2 corpora.
//!
//! The corpora are third-party checkouts and are never committed, so the suite
//! is driven by an environment variable:
//!
//! ```bash
//! REPODEX_TASK2_CORPUS_DIR=~/reposuite/repodex/corpora \
//!   cargo test --locked --test task2_real_incremental
//! ```
//!
//! Without it the test reports the skip and passes, so the committed suite
//! stays green on a machine that has no corpora. A skipped run proves nothing;
//! the TASK 2 report records whether it actually ran.

mod support;

use std::path::{Path, PathBuf};

use repodex::incremental::{run_case, run_sequence, TextEdit};
use repodex::parser::ParserRegistry;
use repodex::LanguageId;
use tree_sitter::Parser;

const ENV: &str = "REPODEX_TASK2_CORPUS_DIR";
/// When set, every executed comparison is written here as one JSON record per
/// file, so the mandatory validation run leaves machine-readable evidence.
const EVIDENCE_ENV: &str = "REPODEX_TASK2_INCREMENTAL_EVIDENCE";

struct Harness {
    registry: ParserRegistry,
    parser: Parser,
    language: LanguageId,
}

impl Harness {
    fn new(language: LanguageId) -> Self {
        let registry = ParserRegistry::new().expect("registry");
        let mut parser = Parser::new();
        parser
            .set_language(&registry.adapter(language).ts_language())
            .expect("grammar must load");
        Self {
            registry,
            parser,
            language,
        }
    }

    fn run(
        &mut self,
        path: &str,
        source: &str,
        edit: &TextEdit,
    ) -> repodex::incremental::IncrementalOutcome {
        let adapter = self.registry.adapter(self.language);
        run_case(adapter, &mut self.parser, path, source, edit).expect("case must run")
    }

    fn run_sequence(
        &mut self,
        path: &str,
        source: &str,
        edits: &[TextEdit],
    ) -> Vec<repodex::incremental::IncrementalOutcome> {
        let adapter = self.registry.adapter(self.language);
        run_sequence(adapter, &mut self.parser, path, source, edits).expect("sequence must run")
    }
}

fn corpus_root() -> Option<PathBuf> {
    let raw = std::env::var(ENV).ok()?;
    if raw.is_empty() {
        return None;
    }
    let path = PathBuf::from(raw);
    path.is_dir().then_some(path)
}

/// Real files, chosen by a rule that does not depend on RepoDex output: the
/// first file of the language whose path sorts first and which is at least
/// 60 lines. A missing file is skipped, not failed, so the suite survives a
/// corpus that has moved on.
fn pick(root: &Path, subdir: &str, extension: &str, want: usize) -> Vec<PathBuf> {
    let base = root.join(subdir);
    let mut found: Vec<PathBuf> = Vec::new();
    collect(&base, extension, &mut found);
    found.sort();
    found
        .into_iter()
        .filter(|p| {
            std::fs::read(p)
                .map(|b| b.iter().filter(|&&c| c == b'\n').count() >= 60)
                .unwrap_or(false)
        })
        .take(want)
        .collect()
}

fn collect(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) != Some(".git") {
                collect(&path, extension, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            out.push(path);
        }
    }
}

/// Byte offset of the start of a line, clamped into the source.
fn line_start(source: &str, line: usize) -> usize {
    source
        .match_indices('\n')
        .nth(line.saturating_sub(1))
        .map(|(i, _)| i + 1)
        .unwrap_or(source.len().saturating_sub(1))
}

fn exercise(
    language: LanguageId,
    label: &str,
    repo: &str,
    path: &Path,
) -> Option<serde_json::Value> {
    let source = std::fs::read_to_string(path).expect("corpus file must be valid UTF-8");
    if source.len() < 400 {
        return None;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let mut harness = Harness::new(language);
    let mid = source.len() / 2;

    // Single edits, each compared against an independent full parse.
    let cases: Vec<(&str, TextEdit)> = vec![
        (
            "line insertion near the top",
            TextEdit::insert(line_start(&source, 2), "// task2 insertion\n"),
        ),
        (
            "line deletion in the middle",
            TextEdit::delete(line_start(&source, 5), line_start(&source, 6)),
        ),
        (
            "small expression edit",
            TextEdit::replace(mid, mid + 1, "x"),
        ),
        (
            "edit near end of file",
            TextEdit::insert(source.len() - 1, "\n// task2 tail\n"),
        ),
        ("temporary syntax damage", TextEdit::insert(mid, "((( ")),
    ];
    let mut comparisons = Vec::new();
    for (what, edit) in cases {
        let outcome = harness.run(name, &source, &edit);
        let equivalent = outcome.equivalent();
        comparisons.push(serde_json::json!({"case": what, "equivalent": equivalent}));
        assert!(
            equivalent,
            "{label} {name}: incremental and fresh results disagree after: {what}"
        );
    }

    // A damage -> repair sequence, so the second edit lands on a tree that was
    // built from damaged bytes.
    let sequence = vec![
        TextEdit::insert(mid, "((( "),
        TextEdit::delete(mid, mid + 4),
        TextEdit::insert(line_start(&source, 3), "// repaired\n"),
    ];
    for (index, outcome) in harness
        .run_sequence(name, &source, &sequence)
        .into_iter()
        .enumerate()
    {
        let equivalent = outcome.equivalent();
        comparisons.push(
            serde_json::json!({"case": format!("sequence step {index}"), "equivalent": equivalent}),
        );
        assert!(
            equivalent,
            "{label} {name}: incremental and fresh results disagree at sequence step {index}"
        );
    }

    Some(serde_json::json!({
        "language": label,
        "repository": repo,
        "file": path.display().to_string(),
        "file_name": name,
        "file_size_bytes": source.len(),
        "comparisons": comparisons,
        "comparison_count": comparisons.len(),
        "result": "equivalent",
    }))
}

#[test]
fn real_files_stay_equivalent_under_incremental_edits() {
    let Some(root) = corpus_root() else {
        eprintln!(
            "SKIPPED: set {ENV} to the TASK 2 corpus directory to run real-file incremental checks"
        );
        return;
    };
    let plan = [
        (LanguageId::Rust, "rust", "tokio-rs_tokio", "rs"),
        (LanguageId::Go, "go", "gin-gonic_gin", "go"),
        (LanguageId::Python, "python", "pallets_flask", "py"),
        (LanguageId::Php, "php", "composer_composer", "php"),
    ];
    let mut records = Vec::new();
    let mut per_language = std::collections::BTreeMap::new();
    for (language, label, repo, extension) in plan {
        let mut executed = 0;
        for path in pick(&root, repo, extension, 3) {
            if let Some(record) = exercise(language, label, repo, &path) {
                records.push(record);
                executed += 1;
            }
        }
        // A file skipped for minimum length is *not* an executed file, so a
        // language with no executed real file is a failure, not a pass.
        assert!(
            executed > 0,
            "{label}: no real file was executed (minimum-length skips do not count)"
        );
        per_language.insert(label, executed);
    }
    if let Ok(path) = std::env::var(EVIDENCE_ENV) {
        let mut out = String::new();
        for record in &records {
            out.push_str(&serde_json::to_string(record).expect("record serialises"));
            out.push('\n');
        }
        std::fs::write(&path, out).expect("evidence must be writable");
    }
    let total: usize = records
        .iter()
        .map(|r| r["comparison_count"].as_u64().unwrap_or(0) as usize)
        .sum();
    eprintln!(
        "real-file incremental: {} files, {} comparisons, per-language {:?}",
        records.len(),
        total,
        per_language
    );
}
