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

fn exercise(language: LanguageId, label: &str, path: &Path) {
    let source = std::fs::read_to_string(path).expect("corpus file must be valid UTF-8");
    if source.len() < 400 {
        return;
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
    for (what, edit) in cases {
        let outcome = harness.run(name, &source, &edit);
        assert!(
            outcome.equivalent(),
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
        assert!(
            outcome.equivalent(),
            "{label} {name}: incremental and fresh results disagree at sequence step {index}"
        );
    }
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
    let mut ran = 0;
    for (language, label, subdir, extension) in plan {
        for path in pick(&root, subdir, extension, 3) {
            exercise(language, label, &path);
            ran += 1;
        }
    }
    assert!(
        ran > 0,
        "the corpus directory contained no usable real files"
    );
    eprintln!("real-file incremental checks ran on {ran} files");
}
