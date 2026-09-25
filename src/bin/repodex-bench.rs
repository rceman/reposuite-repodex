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
use repodex::model::{AnalysisStatus, FileAnalysis, LanguageId};
use repodex::parser::{
    extract_from_tree, Analyzer, AnalyzerConfig, LanguageAdapter, ParserRegistry,
};
use repodex::paths::RepoDexPaths;
use repodex::scanner::{ScanOptions, Scanner};
use serde::Serialize;
use tree_sitter::{InputEdit, Parser, Tree};

const SIZES: [(&str, usize); 5] = [
    ("100KiB", 100 * 1024),
    ("1MiB", 1024 * 1024),
    ("4MiB", 4 * 1024 * 1024),
    ("over-8MiB", 8 * 1024 * 1024 + 4096),
    // Deeply nested source. The depth is bounded by the generator so the
    // grammar and the extraction stack are exercised without unbounded input.
    //
    // The depth is 500 because `tree-sitter-python` fails on deeper nesting: at
    // 513 nested indentation levels it emits a single ERROR node covering the
    // whole file. 500 is the deepest depth that every supported grammar parses
    // clean, which is what this benchmark needs. See `docs/LANGUAGE_SPIKE.md`.
    ("nested", 500),
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
    let rustc = option(&args, "--rustc").unwrap_or_else(|| "see `rustc --version`".to_string());
    lines.push(format!("rustc: {rustc}"));
    lines.push(String::new());
    lines.push(header());

    let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
    let registry = ParserRegistry::new().expect("registry");
    let mut all_rows: Vec<Row> = Vec::new();

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
                all_rows.push(row);
            }
        }
    }

    let report = lines.join("\n") + "\n";
    let report_path = run_dir.join("results.txt");
    if let Err(error) = std::fs::write(&report_path, &report) {
        eprintln!("cannot write {}: {error}", report_path.display());
        std::process::exit(1);
    }
    let json = render_json(
        &run_id,
        &run_dir,
        &rustc,
        &languages,
        &selected_sizes,
        &all_rows,
    );
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

/// The exact unit of work the `incremental parse + extraction` stage times.
///
/// It performs the tree edit, the incremental parse and the whole-file
/// re-extraction, and returns both the resulting tree and the resulting
/// analysis. Naming it makes the interval's contents checkable: whatever this
/// function returns was necessarily produced between the caller's two
/// `Instant::now()` calls, so a test can assert the interval covers the parse
/// and the extraction without comparing durations.
fn incremental_parse_and_extract(
    parser: &mut Parser,
    adapter: &dyn LanguageAdapter,
    file_name: &str,
    base_tree: &Tree,
    edited: &str,
    edit: &InputEdit,
) -> (Tree, FileAnalysis) {
    let mut edited_tree = base_tree.clone();
    edited_tree.edit(edit);
    let tree = parser
        .parse(edited.as_bytes(), Some(&edited_tree))
        .expect("incremental parse");
    let analysis = extract_from_tree(adapter, file_name, edited.as_bytes(), &tree);
    (tree, analysis)
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

    // 5. Full parse + extraction: parse the unedited source and extract from
    //    that tree. The timer covers both, so the label is literally true.
    let start = Instant::now();
    let parsed = parser.parse(source, None).expect("parse");
    analysis = extract_from_tree(adapter, file_name, source.as_bytes(), &parsed);
    let combined = start.elapsed();
    rows.push(row(
        "full parse + extraction",
        language,
        size,
        source.len() as u64,
        source.len() as u64,
        1,
        combined,
        analysis.declarations.len() as u64,
        analysis.calls.len() as u64,
        "parse and extraction both inside the timer",
    ));

    // 6. Incremental parse only: append a language-correct comment, reuse the
    //    old tree. The `edit()` call is setup and is deliberately *outside* the
    //    timer, so this stage measures Tree-sitter's incremental parse and
    //    nothing else. Stage 7 measures the edit as well.
    let edited = append_incremental_tail(language, source);
    assert_ne!(
        edited, source,
        "the incremental edit must actually change the source"
    );
    let edit = tree_sitter::InputEdit {
        start_byte: source.len(),
        old_end_byte: source.len(),
        new_end_byte: edited.len(),
        start_position: point_of(source, source.len()),
        old_end_position: point_of(source, source.len()),
        new_end_position: point_of(&edited, edited.len()),
    };
    let mut edited_tree = parsed.clone();
    edited_tree.edit(&edit);
    let start = Instant::now();
    let incremental_tree = parser
        .parse(edited.as_bytes(), Some(&edited_tree))
        .expect("incremental parse");
    let incremental = start.elapsed();
    // The parse-only stage must still produce a usable tree: a broken edit would
    // otherwise show up only in the combined stage.
    assert!(
        !incremental_tree.root_node().has_error(),
        "the incremental parse-only stage must produce an error-free tree for {}",
        language.as_str()
    );
    rows.push(row(
        "incremental parse only",
        language,
        size,
        edited.len() as u64,
        edited.len() as u64,
        1,
        incremental,
        0,
        0,
        "parse only; the tree edit was prepared outside the timer",
    ));

    // 7. Incremental parse + extraction, which is the TASK 1 policy: after an
    //    incremental parse, re-extract the whole file. The timer starts before
    //    the tree edit and ends after extraction, so it covers the whole
    //    incremental path rather than only its last step.
    //
    //    The timed work is a named function rather than an inline block, so a
    //    test can assert what the interval contains instead of inferring it
    //    from durations: the function returns both the incrementally parsed tree
    //    and the extracted analysis, so anything it returns was produced inside
    //    the interval.
    let start = Instant::now();
    let (incremental_tree, re_extracted) =
        incremental_parse_and_extract(&mut parser, adapter, file_name, &parsed, &edited, &edit);
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
        "tree edit, incremental parse and whole-file re-extraction all inside the timer",
    ));

    // The edit must be valid for this language: if the appended tail were a
    // comment in the wrong syntax, the edited source would be damaged and the
    // incremental measurement would be measuring recovery instead.
    assert_eq!(
        re_extracted.status,
        AnalysisStatus::Clean,
        "the incremental tail must not damage {} source",
        language.as_str()
    );
    assert!(
        !incremental_tree.root_node().has_error(),
        "the incrementally parsed tree must be error-free for {}",
        language.as_str()
    );
    assert_eq!(
        re_extracted.recovery_regions.len(),
        0,
        "the incremental tail must not introduce recovery regions for {}",
        language.as_str()
    );
    // The incrementally parsed tree and a fresh parse of the same bytes must
    // agree structurally, so the measurement is of a real incremental parse and
    // not of a divergent tree.
    let fresh = parser.parse(edited.as_bytes(), None).expect("fresh parse");
    assert_eq!(
        repodex::incremental::tree_structure_digest(
            incremental_tree.root_node(),
            edited.as_bytes()
        ),
        repodex::incremental::tree_structure_digest(fresh.root_node(), edited.as_bytes()),
        "the incremental tree must match a fresh parse for {}",
        language.as_str()
    );
    // And the extraction must agree too.
    let fresh_analysis = extract_from_tree(adapter, file_name, edited.as_bytes(), &fresh);
    assert!(
        re_extracted.same_facts(&fresh_analysis),
        "incremental extraction must match full extraction for {}",
        language.as_str()
    );

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

/// The incremental benchmark edit: append one comment line at EOF.
///
/// The comment syntax must be valid for the language, otherwise the "edit" is
/// really syntax damage and the incremental stage measures error recovery
/// instead of incremental parsing. Python needs `#`; Rust, Go and PHP all use
/// `//`, and the generated PHP source never closes its `<?php` tag, so a `//`
/// line at EOF is a real PHP comment.
///
/// `measure` asserts afterwards that the edited source parses clean, so a wrong
/// comment syntax here fails the run rather than quietly changing what the
/// stage measures.
fn append_incremental_tail(language: LanguageId, source: &str) -> String {
    let comment = match language {
        LanguageId::Python => "# incremental tail",
        LanguageId::Rust | LanguageId::Go | LanguageId::Php => "// incremental tail",
        LanguageId::Manifest => "// incremental tail",
    };
    format!("{source}\n{comment}\n")
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

/// One machine-readable measurement.
///
/// Every field a reader needs to interpret or re-derive the number is present:
/// what ran, on what, how much of it, for how long, and what came out.
#[derive(Serialize)]
struct JsonRow {
    run_id: String,
    language: &'static str,
    size: &'static str,
    stage: &'static str,
    input_bytes: u64,
    processed_bytes: u64,
    files: u64,
    duration_ms: f64,
    /// `None` when the stage processed no bytes, so no rate is meaningful.
    throughput_mib_s: Option<f64>,
    declarations: u64,
    calls: u64,
    note: &'static str,
}

impl JsonRow {
    fn from_row(run_id: &str, row: &Row) -> Self {
        let seconds = row.duration.as_secs_f64();
        let throughput = if seconds > 0.0 && row.processed_bytes > 0 {
            Some(row.processed_bytes as f64 / (1024.0 * 1024.0) / seconds)
        } else {
            None
        };
        Self {
            run_id: run_id.to_string(),
            language: row.language.as_str(),
            size: row.size,
            stage: row.stage,
            input_bytes: row.bytes,
            processed_bytes: row.processed_bytes,
            files: row.files,
            duration_ms: seconds * 1000.0,
            throughput_mib_s: throughput,
            declarations: row.declarations,
            calls: row.calls,
            note: row.note,
        }
    }
}

#[derive(Serialize)]
struct JsonReport {
    schema_version: u32,
    run_id: String,
    output_directory: String,
    rustc: String,
    default_max_file_size: u64,
    languages: Vec<&'static str>,
    sizes: Vec<String>,
    rows: Vec<JsonRow>,
}

/// Machine-readable results: metadata plus one row per measurement.
fn render_json(
    run_id: &str,
    run_dir: &Path,
    rustc: &str,
    languages: &[LanguageId],
    sizes: &[&str],
    rows: &[Row],
) -> String {
    let report = JsonReport {
        schema_version: 1,
        run_id: run_id.to_string(),
        output_directory: run_dir.display().to_string(),
        rustc: rustc.to_string(),
        default_max_file_size: DEFAULT_MAX_FILE_SIZE,
        languages: languages.iter().map(|language| language.as_str()).collect(),
        sizes: sizes.iter().map(|size| (*size).to_string()).collect(),
        rows: rows
            .iter()
            .map(|row| JsonRow::from_row(run_id, row))
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&report).expect("serialize benchmark results");
    text.push('\n');
    text
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
        LanguageId::Manifest => {}
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
            LanguageId::Manifest => {}
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
        LanguageId::Manifest => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    fn analyzer() -> Analyzer {
        Analyzer::new(AnalyzerConfig::default()).expect("analyzer")
    }

    fn analyze(language: LanguageId, source: &str) -> repodex::FileAnalysis {
        analyzer().analyze_bytes(
            &format!("synthetic.{}", language.primary_extension()),
            language,
            source.as_bytes(),
        )
    }

    /// Analyze without the default size limit, so the `over-8MiB` size can be
    /// checked for syntactic cleanliness rather than for size rejection.
    fn analyze_ignoring_size(language: LanguageId, source: &str) -> repodex::FileAnalysis {
        let analyzer = Analyzer::new(AnalyzerConfig {
            max_file_size: source.len() as u64 + 1,
        })
        .expect("analyzer");
        analyzer.analyze_bytes(
            &format!("synthetic.{}", language.primary_extension()),
            language,
            source.as_bytes(),
        )
    }

    /// The generated synthetic source must be clean for every language and size,
    /// otherwise every measurement is really a measurement of error recovery.
    ///
    /// The `nested` size is 600 containers deep, and extraction recurses per
    /// container. A test thread gets a much smaller stack than the main thread,
    /// so the check runs on a thread with an explicit stack: the harness itself
    /// runs on the main thread, where the same depth is fine.
    #[test]
    fn every_generated_source_is_syntactically_clean() {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                for language in LanguageId::ALL {
                    for (size_name, target) in SIZES {
                        let source = generate(language, size_name, target);
                        // The `over-8MiB` size is larger than the default limit
                        // by design, so cleanliness is checked with the limit
                        // raised.
                        let analysis = analyze_ignoring_size(language, &source);
                        assert_eq!(
                            analysis.status,
                            AnalysisStatus::Clean,
                            "generated {} source at {size_name} must be clean",
                            language.as_str()
                        );
                        assert!(
                            analysis.recovery_regions.is_empty(),
                            "generated {} source at {size_name} must have no recovery regions",
                            language.as_str()
                        );
                        assert!(
                            !analysis.declarations.is_empty(),
                            "generated {} source at {size_name} must declare something",
                            language.as_str()
                        );
                    }
                }
            })
            .expect("spawn")
            .join()
            .expect("the generated corpus must be clean");
    }

    /// The incremental edit must be a valid comment in every language.
    ///
    /// A Python source with `//` appended is syntax damage, so the incremental
    /// stage would silently become an error-recovery measurement. This is the
    /// regression test for that.
    #[test]
    fn the_incremental_tail_is_valid_for_every_language() {
        for language in LanguageId::ALL {
            let source = generate(language, "100KiB", 100 * 1024);
            let edited = append_incremental_tail(language, &source);

            // The edit is an append, so the original bytes are untouched.
            assert!(edited.starts_with(&source));
            assert!(edited.len() > source.len());
            assert!(edited.ends_with("incremental tail\n"));

            let analysis = analyze(language, &edited);
            assert_eq!(
                analysis.status,
                AnalysisStatus::Clean,
                "the incremental tail must not damage {} source",
                language.as_str()
            );
            assert!(
                analysis.recovery_regions.is_empty(),
                "the incremental tail must not introduce recovery regions for {}",
                language.as_str()
            );

            // A comment adds no facts, so the fact set is unchanged.
            let before = analyze(language, &source);
            assert_eq!(
                analysis.declarations.len(),
                before.declarations.len(),
                "the tail must not add or remove declarations for {}",
                language.as_str()
            );
            assert_eq!(analysis.calls.len(), before.calls.len());
            assert_eq!(analysis.imports.len(), before.imports.len());
        }
    }

    /// `tree-sitter-python` has a real depth limit, and the benchmark's nested
    /// size must stay below it.
    ///
    /// At 513 nested indentation levels `tree-sitter-python` emits one ERROR
    /// node covering the whole file, so a deeper benchmark would be measuring
    /// error recovery while claiming to measure nested parsing. The exact
    /// boundary is a property of the pinned grammar, so this test pins only the
    /// two facts the harness depends on: the benchmark depth is clean, and a
    /// clearly deeper depth is not.
    #[test]
    fn the_nested_benchmark_depth_is_within_every_grammar_limit() {
        let depth = SIZES
            .iter()
            .find(|(name, _)| *name == "nested")
            .expect("the nested size")
            .1;
        assert!(depth <= 512, "the benchmark depth must be inside the limit");

        let source = generate(LanguageId::Python, "nested", depth);
        let analysis = analyze_ignoring_size(LanguageId::Python, &source);
        assert_eq!(
            analysis.status,
            AnalysisStatus::Clean,
            "the nested benchmark depth must parse clean in Python"
        );

        // A depth well past the limit must still fail, so the guard cannot be
        // satisfied by a grammar that simply accepts anything.
        let deeper = generate_nested(LanguageId::Python, 600);
        let deeper_analysis = analyze_ignoring_size(LanguageId::Python, &deeper);
        assert_ne!(
            deeper_analysis.status,
            AnalysisStatus::Clean,
            "tree-sitter-python is expected to fail on very deep indentation; if this              now succeeds, the documented limit and the benchmark depth can be raised"
        );
    }

    /// The comment syntax must be the language's own.
    #[test]
    fn the_incremental_tail_uses_the_language_comment_syntax() {
        let python = append_incremental_tail(LanguageId::Python, "x = 1\n");
        assert!(python.contains("# incremental tail"), "{python}");
        assert!(
            !python.contains("// incremental tail"),
            "Python has no `//` comment: {python}"
        );

        for language in [LanguageId::Rust, LanguageId::Go, LanguageId::Php] {
            let edited = append_incremental_tail(language, "x\n");
            assert!(
                edited.contains("// incremental tail"),
                "{} uses `//`: {edited}",
                language.as_str()
            );
        }
    }

    /// The incremental stages must measure the edited source, and the combined
    /// stage must include the extraction.
    ///
    /// What a test can prove here is deterministic and structural:
    ///
    /// * the parse-only stage reports no facts, so it does not extract;
    /// * the combined stage reports the facts of the edited source, so
    ///   extraction happened inside it;
    /// * both stages measured the edited bytes, and the full stage measured the
    ///   unedited bytes;
    /// * `incremental_parse_and_extract` — the single call the combined stage
    ///   times — returns both a tree and an analysis, so both were produced
    ///   inside the interval.
    ///
    /// What no test here proves is the *parse* half of the combined interval by
    /// timing: "combined took longer than the parse alone" is a timing
    /// inequality and would not demonstrate that the parse is inside the timer
    /// even when it happens to hold. The interval's start point is established
    /// by code structure — the timer opens immediately before the call above
    /// and closes immediately after it — and that call is what this test
    /// inspects. There is no flaky timing assertion here on purpose.
    #[test]
    fn the_incremental_stages_measure_the_edited_source_and_the_combined_stage_extracts() {
        let work = TempDir::new("bench-stages");
        let source = generate(LanguageId::Rust, "1MiB", 1024 * 1024);
        let path = work.write("stage.rs", source.as_bytes());
        let registry = ParserRegistry::new().expect("registry");
        let rows = measure(
            &analyzer(),
            &registry,
            LanguageId::Rust,
            "1MiB",
            "stage.rs",
            &path,
            &source,
        );

        let find = |stage: &str| {
            rows.iter()
                .find(|row| row.stage == stage)
                .unwrap_or_else(|| {
                    panic!(
                        "missing stage `{stage}`; stages present: {:?}",
                        rows.iter().map(|row| row.stage).collect::<Vec<_>>()
                    )
                })
        };

        let parse_only = find("incremental parse only");
        let combined = find("incremental parse + extraction");
        let full = find("full parse + extraction");

        // The parse-only stage reports no facts, because it does not extract.
        assert_eq!(parse_only.declarations, 0);
        assert_eq!(parse_only.calls, 0);

        // The combined stage reports the facts of the edited source, so
        // extraction happened inside the stage.
        assert!(combined.declarations > 0);
        assert!(combined.calls > 0);

        // Both incremental stages measured the edited source; the full stage
        // measured the unedited source.
        let edited = append_incremental_tail(LanguageId::Rust, &source);
        assert_eq!(parse_only.bytes, edited.len() as u64);
        assert_eq!(combined.bytes, edited.len() as u64);
        assert_eq!(full.bytes, source.len() as u64);

        // The timed unit itself: one call, two outputs. A tree and an analysis
        // can only come out of it if both the parse and the extraction ran
        // inside it.
        let mut parser = Parser::new();
        parser
            .set_language(&registry.adapter(LanguageId::Rust).ts_language())
            .expect("grammar");
        let base = parser.parse(&source, None).expect("base parse");
        let edit = InputEdit {
            start_byte: source.len(),
            old_end_byte: source.len(),
            new_end_byte: edited.len(),
            start_position: point_of(&source, source.len()),
            old_end_position: point_of(&source, source.len()),
            new_end_position: point_of(&edited, edited.len()),
        };
        let (tree, analysis) = incremental_parse_and_extract(
            &mut parser,
            registry.adapter(LanguageId::Rust),
            "stage.rs",
            &base,
            &edited,
            &edit,
        );

        // The tree is a real parse of the edited bytes: it agrees structurally
        // with a fresh parse of the same bytes, and it is not the base tree.
        let fresh = parser.parse(&edited, None).expect("fresh parse");
        assert_eq!(
            repodex::incremental::tree_structure_digest(tree.root_node(), edited.as_bytes()),
            repodex::incremental::tree_structure_digest(fresh.root_node(), edited.as_bytes()),
            "the timed parse must produce the tree of the edited source"
        );
        assert_ne!(
            repodex::incremental::tree_structure_digest(tree.root_node(), edited.as_bytes()),
            repodex::incremental::tree_structure_digest(base.root_node(), source.as_bytes()),
            "the timed parse must not just return the base tree"
        );

        // The analysis is a real extraction of the edited bytes: it carries
        // facts and agrees with a fresh extraction.
        assert!(!analysis.declarations.is_empty());
        assert!(!analysis.calls.is_empty());
        assert_eq!(analysis.status, AnalysisStatus::Clean);
        let fresh_analysis = extract_from_tree(
            registry.adapter(LanguageId::Rust),
            "stage.rs",
            edited.as_bytes(),
            &fresh,
        );
        assert!(
            analysis.same_facts(&fresh_analysis),
            "the timed extraction must match a fresh extraction of the same bytes"
        );
    }

    /// The JSON artifact must carry the measurements, not just a pointer to them.
    #[test]
    fn the_json_report_contains_one_row_per_measurement() {
        let rows = vec![
            row(
                "raw parse",
                LanguageId::Go,
                "100KiB",
                1024,
                1024,
                1,
                Duration::from_millis(5),
                0,
                0,
                "test row",
            ),
            row(
                "incremental parse + extraction",
                LanguageId::Go,
                "100KiB",
                2048,
                2048,
                1,
                Duration::from_millis(10),
                7,
                9,
                "test row two",
            ),
        ];
        let json = render_json(
            "test-run",
            Path::new("/tmp/test-run"),
            "rustc test",
            &[LanguageId::Go],
            &["100KiB"],
            &rows,
        );
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

        assert_eq!(parsed["run_id"], "test-run");
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["rustc"], "rustc test");
        assert_eq!(parsed["default_max_file_size"], DEFAULT_MAX_FILE_SIZE);
        assert_eq!(parsed["languages"][0], "go");
        assert_eq!(parsed["sizes"][0], "100KiB");

        let rows = parsed["rows"].as_array().expect("rows array");
        assert_eq!(rows.len(), 2, "one row per measurement");
        let first = &rows[0];
        for key in [
            "run_id",
            "language",
            "size",
            "stage",
            "input_bytes",
            "processed_bytes",
            "files",
            "duration_ms",
            "throughput_mib_s",
            "declarations",
            "calls",
            "note",
        ] {
            assert!(!first[key].is_null(), "row is missing `{key}`: {first}");
        }
        assert_eq!(first["run_id"], "test-run");
        assert_eq!(first["language"], "go");
        assert_eq!(first["stage"], "raw parse");
        assert_eq!(first["input_bytes"], 1024);
        assert_eq!(first["files"], 1);
        assert_eq!(first["note"], "test row");
        // The second row carries the fact counts and a throughput, so a reader
        // can re-derive the number without the text report.
        assert_eq!(rows[1]["declarations"], 7);
        assert_eq!(rows[1]["calls"], 9);
        assert!((rows[1]["duration_ms"].as_f64().expect("number") - 10.0).abs() < 1e-6);
        // 2048 bytes in 10 ms is 2048 / 1048576 / 0.01 MiB/s.
        let expected = 2048.0 / (1024.0 * 1024.0) / 0.01;
        assert!(
            (rows[1]["throughput_mib_s"].as_f64().expect("number") - expected).abs() < 1e-6,
            "{}",
            rows[1]["throughput_mib_s"]
        );
    }

    /// A stage that processed no bytes must not claim a throughput.
    #[test]
    fn a_stage_without_processed_bytes_reports_no_throughput() {
        let rows = vec![row(
            "over default max size",
            LanguageId::Rust,
            "over-8MiB",
            9000,
            0,
            1,
            Duration::from_millis(1),
            0,
            0,
            "rejected",
        )];
        let json = render_json(
            "test-run",
            Path::new("/tmp/test-run"),
            "rustc test",
            &[LanguageId::Rust],
            &["over-8MiB"],
            &rows,
        );
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert!(
            parsed["rows"][0]["throughput_mib_s"].is_null(),
            "no bytes processed means no rate: {}",
            parsed["rows"][0]
        );
    }
}
