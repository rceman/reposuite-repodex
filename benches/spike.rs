//! `cargo bench` harness for the committed fixture corpus.
//!
//! This measures the same pipeline stages as `src/bin/repodex-bench.rs`, but
//! over the checked-in fixtures instead of generated synthetic sizes, so it
//! needs no arguments, no output directory and no large generated input:
//!
//! ```text
//! cargo bench
//! ```
//!
//! The synthetic size matrix, the incremental stages and the over-8-MiB limit
//! behaviour live in `repodex-bench`. Neither harness compares RepoDex against
//! another implementation; both report only observed durations.
//!
//! Every row is a mean over repeated iterations. `docs/SPIKE_RESULTS.md` records
//! the exact command and the observed numbers.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use repodex::parser::{extract_from_tree, Analyzer, AnalyzerConfig, ParserRegistry};
use repodex::LanguageId;
use tree_sitter::Parser;

/// Iterations per measurement. Enough to be stable, small enough to stay quick.
const ITERATIONS: u32 = 20;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
    let registry = ParserRegistry::new().expect("registry");

    println!(
        "{:<10} {:<32} {:>10} {:>6} {:>12} {:>10} {:>13} {:>8}",
        "language", "fixture", "bytes", "iters", "ms/iter", "MiB/s", "declarations", "calls"
    );

    for language in LanguageId::ALL {
        let adapter = registry.adapter(language);
        let mut parser = Parser::new();
        parser
            .set_language(&adapter.ts_language())
            .expect("grammar must load");

        for relative in fixtures_for(&root, language) {
            let path = root.join("fixtures").join(&relative);
            let source = std::fs::read(&path).expect("fixture must be readable");
            let source_text = String::from_utf8(source.clone()).expect("fixtures are UTF-8");
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("file name");

            // Warm up so grammar loading and allocation are not measured.
            let tree = parser.parse(&source_text, None).expect("parse");
            let analysis = extract_from_tree(adapter, file_name, &source, &tree);

            let parse_time = mean(ITERATIONS, || {
                std::hint::black_box(parser.parse(&source_text, None).expect("parse"));
            });
            let extract_time = mean(ITERATIONS, || {
                std::hint::black_box(extract_from_tree(adapter, file_name, &source, &tree));
            });
            let combined = parse_time + extract_time;
            let seconds = combined.as_secs_f64();
            let mib = source.len() as f64 / (1024.0 * 1024.0);
            println!(
                "{:<10} {:<32} {:>10} {:>6} {:>12.3} {:>10.2} {:>13} {:>8}",
                language.as_str(),
                relative,
                source.len(),
                ITERATIONS,
                seconds * 1000.0,
                if seconds > 0.0 { mib / seconds } else { 0.0 },
                analysis.declarations.len(),
                analysis.calls.len(),
            );
        }
    }

    // One end-to-end scan of the whole fixture tree.
    let scanner =
        repodex::scanner::Scanner::new(&analyzer, repodex::scanner::ScanOptions::default());
    let start = Instant::now();
    let report = scanner
        .scan(&root.join("fixtures"))
        .expect("fixtures directory must be scannable");
    let elapsed = start.elapsed();
    println!();
    println!(
        "fixture tree scan: {} files, {} supported, {} clean, {} recovered, {} bytes in {:.3} ms \
         ({:.2} MiB/s)",
        report.visited_files,
        report.supported_files,
        report.parsed_clean_files,
        report.parsed_with_recovery_files,
        report.bytes_processed,
        elapsed.as_secs_f64() * 1000.0,
        if elapsed.as_secs_f64() > 0.0 {
            report.bytes_processed as f64 / (1024.0 * 1024.0) / elapsed.as_secs_f64()
        } else {
            0.0
        }
    );
    println!(
        "canonical scan digest: {}",
        repodex::canonical::scan_digest(&report)
    );
    println!();
    println!("Synthetic size matrix, incremental stages and the 8 MiB limit: `repodex-bench`.");
}

fn mean(iterations: u32, mut body: impl FnMut()) -> Duration {
    // One untimed iteration keeps the measured loop free of first-call effects.
    body();
    let start = Instant::now();
    for _ in 0..iterations {
        body();
    }
    start.elapsed() / iterations
}

fn fixtures_for(root: &Path, language: LanguageId) -> Vec<String> {
    let directory = root.join("fixtures").join(language.as_str());
    let mut found = Vec::new();
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(_) => return found,
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let matches_extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| language.extensions().contains(&extension))
            .unwrap_or(false);
        if matches_extension {
            found.push(format!(
                "{}/{}",
                language.as_str(),
                path.file_name()
                    .and_then(|name| name.to_str())
                    .expect("file name")
            ));
        }
    }
    found.sort();
    found
}
