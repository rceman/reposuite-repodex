//! RepoDex benchmark harness.
//!
//! Measures the TASK 1 pipeline stages separately where the API allows it, using
//! deterministic synthetic source that contains real extractable constructs.
//!
//! ```text
//! cargo run --release --bin repodex-bench -- --language all --size all
//! ```
//!
//! Output goes to `<REPOSUITE_REPODEX_HOME or ~/reposuite/repodex>/benchmarks/`.
//! Generated sources live in a temporary directory and are removed afterwards.
//!
//! The harness reports only what it observed. It never compares against another
//! implementation and never extrapolates.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use repodex::input::{self, DEFAULT_MAX_FILE_SIZE};
use repodex::model::{AnalysisStatus, LanguageId};
use repodex::parser::{extract_from_tree, Analyzer, AnalyzerConfig, ParserRegistry};
use repodex::paths::RepoDexPaths;
use repodex::scanner::{ScanOptions, Scanner};
use tree_sitter::Parser;

const SIZES: [(&str, usize); 5] = [
    ("100KiB", 100 * 1024),
    ("1MiB", 1024 * 1024),
    ("4MiB", 4 * 1024 * 1024),
    ("over-8MiB", 8 * 1024 * 1024 + 4096),
    // Deeply nested source. The depth is bounded by the generator so the
    // grammar and the extraction stack are exercised without unbounded input.
    ("nested", 600),
];

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        return;
    }
    let languages = match option(&args, "--language").as_deref() {
        None | Some("all") => LanguageId::ALL.to_vec(),
        Some(name) => match LanguageId::from_name(name) {
            Some(language) => vec![language],
            None => {
                eprintln!("unknown language `{name}`");
                std::process::exit(2);
            }
        },
    };
    let selected_sizes: Vec<&str> = match option(&args, "--size").as_deref() {
        None | Some("all") => SIZES.iter().map(|(name, _)| *name).collect(),
        Some("standard") => SIZES[..3].iter().map(|(name, _)| *name).collect(),
        Some(name) => vec![
            SIZES
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .unwrap_or_else(|| {
                    eprintln!("unknown size `{name}`; known sizes: {}", size_names());
                    std::process::exit(2);
                })
                .0,
        ],
    };

    let output_root = match option(&args, "--output") {
        Some(path) => PathBuf::from(path),
        None => default_output_root(),
    };
    let run_id = match option(&args, "--run-id") {
        Some(id) => id,
        None => format!(
            "run-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0)
        ),
    };
    let run_dir = output_root.join(&run_id);
    if let Err(error) = std::fs::create_dir_all(&run_dir) {
        eprintln!("cannot create {}: {error}", run_dir.display());
        std::process::exit(1);
    }

    let work = TempDir::new("repodex-bench");
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("# RepoDex benchmark run {run_id}"));
    lines.push(String::new());
    lines.push(format!("output directory: {}", run_dir.display()));
    lines.push(format!("working directory: {}", work.path().display()));
    lines.push(format!(
        "default max file size: {DEFAULT_MAX_FILE_SIZE} bytes"
    ));
    lines.push(format!(
        "rustc: {}",
        option(&args, "--rustc").unwrap_or_else(|| "see `rustc --version`".to_string())
    ));
    lines.push(String::new());
    lines.push(header());

    let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
    let registry = ParserRegistry::new().expect("registry");

    for language in &languages {
        for size_name in &selected_sizes {
            let bytes = SIZES
                .iter()
                .find(|(name, _)| name == size_name)
                .expect("size")
                .1;
            let source = generate(*language, size_name, bytes);
            let extension = language.primary_extension();
            let file_name = format!("synthetic_{size_name}.{extension}");
            // Each size gets its own directory so the discovery measurement is a
            // single-file scan and never includes any other generated size.
            let relative = format!("{}/{size_name}/{file_name}", language.as_str());
            let path = work.write(&relative, source.as_bytes());
            let rows = measure(
                &analyzer, &registry, *language, size_name, &file_name, &path, &source,
            );
            for row in rows {
                lines.push(row.render());
            }
        }
    }

    let report = lines.join("\n") + "\n";
    let report_path = run_dir.join("results.txt");
    if let Err(error) = std::fs::write(&report_path, &report) {
        eprintln!("cannot write {}: {error}", report_path.display());
        std::process::exit(1);
    }
    let json = render_json(&run_id, &run_dir, &languages, &selected_sizes);
    let json_path = run_dir.join("results.json");
    if let Err(error) = std::fs::write(&json_path, &json) {
        eprintln!("cannot write {}: {error}", json_path.display());
        std::process::exit(1);
    }

    print!("{report}");
    println!("written: {}", report_path.display());
    println!("written: {}", json_path.display());
}

fn print_usage() {
    println!(
        "repodex-bench [--language <rust|go|python|php|all>] [--size <{}|all|standard>]\n\
         \x20             [--output <dir>] [--run-id <id>]",
        size_names()
    );
    println!();
    println!(
        "Sizes: {} (`nested` is deeply nested source, 600 levels).",
        size_names()
    );
    println!("`over-8MiB` exceeds the default maximum file size, so the default");
    println!("configuration skips it and the explicit-override row measures the");
    println!("raised limit.");
}

fn size_names() -> String {
    SIZES
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join("|")
}

fn default_output_root() -> PathBuf {
    match RepoDexPaths::from_env() {
        Some(paths) => paths.benchmarks_dir(),
        None => std::env::temp_dir().join("repodex-benchmarks"),
    }
}

fn option(args: &[String], name: &str) -> Option<String> {
    let mut iterator = args.iter();
    while let Some(arg) = iterator.next() {
        if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
            return Some(value.to_string());
        }
        if arg == name {
            return iterator.next().cloned();
        }
    }
    None
}

struct Row {
    stage: &'static str,
    language: LanguageId,
    size: &'static str,
    /// Size of the input this stage was given.
    bytes: u64,
    /// Bytes the stage actually processed. Zero means nothing was parsed or
    /// read, so no throughput can be reported for it.
    processed_bytes: u64,
    files: u64,
    duration: Duration,
    declarations: u64,
    calls: u64,
    note: &'static str,
}

impl Row {
    fn render(&self) -> String {
        let seconds = self.duration.as_secs_f64();
        let mib = self.processed_bytes as f64 / (1024.0 * 1024.0);
        let milliseconds = if seconds > 0.0 {
            format!("{:.3}", seconds * 1000.0)
        } else {
            "-".to_string()
        };
        let throughput = if seconds > 0.0 && self.processed_bytes > 0 {
            format!("{:.2}", mib / seconds)
        } else {
            "-".to_string()
        };
        format!(
            "{:<34} {:<7} {:<9} {:>10} {:>6} {:>12} {:>10} {:>13} {:>8} {}",
            self.stage,
            self.language.as_str(),
            self.size,
            self.bytes,
            self.files,
            milliseconds,
            throughput,
            self.declarations,
            self.calls,
            self.note
        )
    }
}

fn header() -> String {
    format!(
        "{:<34} {:<7} {:<9} {:>10} {:>6} {:>12} {:>10} {:>13} {:>8} {}",
        "stage", "lang", "size", "bytes", "files", "ms", "MiB/s", "declarations", "calls", "note"
    )
}

#[allow(clippy::too_many_arguments)]
fn row(
    stage: &'static str,
    language: LanguageId,
    size: &'static str,
    bytes: u64,
    processed_bytes: u64,
    files: u64,
    duration: Duration,
    declarations: u64,
    calls: u64,
    note: &'static str,
) -> Row {
    Row {
        stage,
        language,
        size,
        bytes,
        processed_bytes,
        files,
        duration,
        declarations,
        calls,
        note,
    }
}

#[allow(clippy::too_many_arguments)]
fn measure(
    analyzer: &Analyzer,
    registry: &ParserRegistry,
    language: LanguageId,
    size: &'static str,
    file_name: &str,
    path: &Path,
    source: &str,
) -> Vec<Row> {
    let mut rows = Vec::new();

    // 1. File discovery: a scan of a one-file directory, so the walk cost is
    //    visible but not dominated by unrelated files.
    let directory = path.parent().expect("parent");
    let scanner = Scanner::new(analyzer, ScanOptions::default());
    let start = Instant::now();
    let report = scanner.scan(directory).expect("scan");
    let discovery = start.elapsed();
    rows.push(row(
        "discovery+read+parse+extract",
        language,
        size,
        source.len() as u64,
        report.bytes_processed,
        1,
        discovery,
        report.declarations,
        report.call_like,
        "scan of a single-file directory",
    ));

    // 2. Parser initialization and first parse.
    let mut parser = Parser::new();
    let start = Instant::now();
    let adapter = registry.adapter(language);
    parser
        .set_language(&adapter.ts_language())
        .expect("grammar must load");
    let tree = parser.parse(source, None).expect("parse");
    let first_parse = start.elapsed();
    rows.push(row(
        "parser init + first parse",
        language,
        size,
        source.len() as u64,
        source.len() as u64,
        1,
        first_parse,
        0,
        0,
        "includes grammar load",
    ));

    // 3. Raw parsing only, warm parser, no extraction.
    let iterations = raw_iterations(source.len());
    let start = Instant::now();
    for _ in 0..iterations {
        std::hint::black_box(parser.parse(source, None).expect("parse"));
    }
    let raw = start.elapsed() / iterations;
    rows.push(row(
        "raw parse",
        language,
        size,
        source.len() as u64,
        source.len() as u64,
        1,
        raw,
        0,
        0,
        "mean of repeated parses, no extraction",
    ));

    // 4. Normalized extraction only, reusing a parsed tree.
    let start = Instant::now();
    let mut analysis = extract_from_tree(adapter, file_name, source.as_bytes(), &tree);
    let extract = start.elapsed();
    rows.push(row(
        "normalized extraction",
        language,
        size,
        source.len() as u64,
        source.len() as u64,
        1,
        extract,
        analysis.declarations.len() as u64,
        analysis.calls.len() as u64,
        "extraction only, tree already parsed",
    ));

    // 5. Parse + extraction.
    let start = Instant::now();
    let parsed = parser.parse(source, None).expect("parse");
    analysis = extract_from_tree(adapter, file_name, source.as_bytes(), &parsed);
    let combined = start.elapsed();
    rows.push(row(
        "parse + extraction",
        language,
        size,
        source.len() as u64,
        source.len() as u64,
        1,
        combined,
        analysis.declarations.len() as u64,
        analysis.calls.len() as u64,
        "single file, warm parser",
    ));

    // 6. Incremental parse: append a comment, reuse the old tree.
    let edited = format!("{source}\n// incremental tail\n");
    let mut edited_tree = parsed.clone();
    edited_tree.edit(&tree_sitter::InputEdit {
        start_byte: source.len(),
        old_end_byte: source.len(),
        new_end_byte: edited.len(),
        start_position: point_of(source, source.len()),
        old_end_position: point_of(source, source.len()),
        new_end_position: point_of(&edited, edited.len()),
    });
    let start = Instant::now();
    let incremental_tree = parser
        .parse(edited.as_bytes(), Some(&edited_tree))
        .expect("incremental parse");
    let incremental = start.elapsed();
    rows.push(row(
        "incremental parse",
        language,
        size,
        edited.len() as u64,
        edited.len() as u64,
        1,
        incremental,
        0,
        0,
        "append at EOF reusing the old tree",
    ));

    // 7. Incremental parse + full-file re-extraction, which is the TASK 1
    //    policy: after an incremental parse, re-extract the whole file.
    let start = Instant::now();
    let re_extracted = extract_from_tree(adapter, file_name, edited.as_bytes(), &incremental_tree);
    let incremental_combined = start.elapsed();
    rows.push(row(
        "incremental parse + extraction",
        language,
        size,
        edited.len() as u64,
        edited.len() as u64,
        1,
        incremental_combined,
        re_extracted.declarations.len() as u64,
        re_extracted.calls.len() as u64,
        "whole-file re-extraction after incremental parse",
    ));

    // 8. The default maximum file size path: skipped or accepted.
    if source.len() as u64 > DEFAULT_MAX_FILE_SIZE {
        let start = Instant::now();
        let read = input::read_source(path, DEFAULT_MAX_FILE_SIZE);
        let rejected = start.elapsed();
        // The read stops as soon as the limit is exceeded, so the bytes
        // actually processed are the truncated read length, not the file size.
        let observed = match &read {
            Err(input::InputError::TooLarge { observed, .. }) => *observed,
            _ => 0,
        };
        rows.push(row(
            "over default max size",
            language,
            size,
            source.len() as u64,
            observed,
            1,
            rejected,
            0,
            0,
            "rejected under the default 8 MiB limit",
        ));
        assert!(read.is_err(), "the default limit must reject this file");
        let raised = Analyzer::new(AnalyzerConfig {
            max_file_size: source.len() as u64 + 1,
        })
        .expect("analyzer");
        let start = Instant::now();
        let analysis = raised.analyze_path(path.parent().expect("parent"), path);
        let elapsed = start.elapsed();
        rows.push(row(
            "over default max size (raised)",
            language,
            size,
            source.len() as u64,
            source.len() as u64,
            1,
            elapsed,
            analysis.declarations.len() as u64,
            analysis.calls.len() as u64,
            "explicit --max-file-size equivalent",
        ));
        assert_eq!(
            analysis.status,
            AnalysisStatus::Clean,
            "raised limit must parse the synthetic source"
        );
    } else {
        let start = Instant::now();
        let read = input::read_source(path, DEFAULT_MAX_FILE_SIZE);
        let accepted = start.elapsed();
        assert!(
            read.is_ok(),
            "file inside the default limit must be readable"
        );
        rows.push(row(
            "under default max size",
            language,
            size,
            source.len() as u64,
            source.len() as u64,
            1,
            accepted,
            0,
            0,
            "read accepted under the default 8 MiB limit",
        ));
    }

    rows
}

fn raw_iterations(bytes: usize) -> u32 {
    // Keep the raw-parse stage bounded: about 8 MiB of parsing work per row.
    let by_size = (8 * 1024 * 1024 / bytes.max(1)).clamp(1, 32);
    by_size as u32
}

fn point_of(text: &str, byte: usize) -> tree_sitter::Point {
    let prefix = &text[..byte];
    let row = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let column = prefix
        .rfind('\n')
        .map(|index| prefix.len() - index - 1)
        .unwrap_or(prefix.len());
    tree_sitter::Point { row, column }
}

fn render_json(run_id: &str, run_dir: &Path, languages: &[LanguageId], sizes: &[&str]) -> String {
    let languages = languages
        .iter()
        .map(|language| format!("\"{}\"", language.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    let sizes = sizes
        .iter()
        .map(|size| format!("\"{size}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\n  \"run_id\": \"{run_id}\",\n  \"output_directory\": \"{}\",\n  \
         \"languages\": [{languages}],\n  \"sizes\": [{sizes}],\n  \
         \"results\": \"results.txt\",\n  \
         \"note\": \"measurements only; no comparison against another implementation\"\n}}\n",
        run_dir.display()
    )
}

/// Deterministic synthetic source with real extractable constructs.
fn generate(language: LanguageId, size_name: &str, target: usize) -> String {
    if size_name == "nested" {
        return generate_nested(language, target);
    }

    let mut source = String::with_capacity(target + 4096);
    match language {
        LanguageId::Rust => source.push_str("// generated synthetic Rust source\n\n"),
        LanguageId::Go => {
            source.push_str("// generated synthetic Go source\npackage synthetic\n\n")
        }
        LanguageId::Python => source.push_str("\"\"\"Generated synthetic Python source.\"\"\"\n\n"),
        LanguageId::Php => source.push_str("<?php\n\n// generated synthetic PHP source\n\n"),
    }
    let mut index = 0usize;
    while source.len() < target {
        index += 1;
        match language {
            LanguageId::Rust => {
                source.push_str(&format!(
                    "pub struct Widget{index} {{\n    pub field_a: u32,\n    pub field_b: String,\n}}\n\n\
                     pub trait Handler{index} {{\n    fn handle(&self) -> u32;\n}}\n\n\
                     impl Widget{index} {{\n    pub fn compute(&self, value: u32) -> u32 {{\n        \
                     let local = value + {index};\n        helper{index}(local)\n    }}\n}}\n\n\
                     impl Handler{index} for Widget{index} {{\n    fn handle(&self) -> u32 {{\n        \
                     self.compute({index})\n    }}\n}}\n\n\
                     pub fn helper{index}(input: u32) -> u32 {{\n    input * 2\n}}\n\n\
                     pub mod module{index} {{\n    pub fn nested() -> u32 {{\n        \
                     super::helper{index}({index})\n    }}\n}}\n\n"
                ));
            }
            LanguageId::Go => {
                source.push_str(&format!(
                    "type Widget{index} struct {{\n\tFieldA int\n\tFieldB string\n}}\n\n\
                     type Handler{index} interface {{\n\tHandle() int\n}}\n\n\
                     func (w Widget{index}) Compute(value int) int {{\n\tlocal := value + {index}\n\t\
                     return helper{index}(local)\n}}\n\n\
                     func helper{index}(input int) int {{\n\treturn input * 2\n}}\n\n\
                     const Status{index} = {index}\n\n\
                     var Global{index} = Widget{index}{{FieldA: {index}}}\n\n"
                ));
            }
            LanguageId::Python => {
                source.push_str(&format!(
                    "class Widget{index}:\n    field_a = {index}\n\n    def compute(self, value):\n        \
                     local = value + {index}\n        return helper{index}(local)\n\n\n\
                     def helper{index}(input_value):\n    return input_value * 2\n\n\n\
                     MODULE_{index} = {index}\n\n\n"
                ));
            }
            LanguageId::Php => {
                source.push_str(&format!(
                    "class Widget{index}\n{{\n    public const LIMIT = {index};\n\n    \
                     public function compute(int $value): int\n    {{\n        $local = $value + {index};\n        \
                     return helper{index}($local);\n    }}\n}}\n\n\
                     function helper{index}(int $input): int\n{{\n    return $input * 2;\n}}\n\n"
                ));
            }
        }
    }
    source
}

/// Deeply nested source, still syntactically valid. `depth` is the number of
/// nested containers.
fn generate_nested(language: LanguageId, depth: usize) -> String {
    let mut source = String::new();
    match language {
        LanguageId::Rust => {
            source.push_str("// generated deeply nested Rust source\n\n");
            for level in 0..depth {
                source.push_str(&format!("pub mod level_{level} {{\n"));
            }
            source.push_str("pub fn deepest() -> u32 {\n    0\n}\n");
            for _ in 0..depth {
                source.push_str("}\n");
            }
        }
        LanguageId::Go => {
            source.push_str("// generated deeply nested Go source\npackage nested\n\n");
            source.push_str("func outer() {\n");
            for level in 0..depth {
                source.push_str(&format!("nested{level} := func() {{\n"));
            }
            source.push_str("_ = 0\n");
            for _ in 0..depth {
                source.push_str("}\n");
            }
            source.push_str("}\n");
        }
        LanguageId::Python => {
            source.push_str("\"\"\"Generated deeply nested Python source.\"\"\"\n\n");
            for level in 0..depth {
                source.push_str(&format!("{}def level_{level}():\n", "    ".repeat(level)));
            }
            source.push_str(&format!("{}return 0\n", "    ".repeat(depth)));
        }
        LanguageId::Php => {
            source.push_str("<?php\n\n// generated deeply nested PHP source\n\n");
            for level in 0..depth {
                source.push_str(&format!("function level_{level}()\n{{\n"));
            }
            source.push_str("return 0;\n");
            for _ in 0..depth {
                source.push_str("}\n");
            }
        }
    }
    source
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create bench work dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, contents: &[u8]) -> PathBuf {
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
