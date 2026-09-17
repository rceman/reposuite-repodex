//! TASK 2 tree/source retention experiment.
//!
//! Parses and extracts every supported file in a corpus directory, then reports
//! the peak resident set size of the whole process. Three variants differ only
//! in what is kept alive after each file's facts are produced:
//!
//! ```text
//! A  release the tree and the source buffer after extraction
//! B  retain the trees, release the source buffers
//! C  retain the trees and the source buffers
//! ```
//!
//! The fact-extraction policy is identical in all three. Run each variant in its
//! own process so one variant's peak does not leak into another:
//!
//! ```bash
//! cargo run --release --example retention -- <corpus-dir> A
//! ```
//!
//! The peak is read from `/proc/self/status` `VmHWM`, which is the kernel's
//! high-water mark for the process. It is the whole process, not a measurement
//! of Tree-sitter objects, and the differences between variants are not exact
//! tree sizes: they include allocator behaviour and the source buffers.

use std::path::{Path, PathBuf};

use repodex::model::LanguageId;
use repodex::parser::{extract_from_tree, ParserRegistry};
use tree_sitter::{Parser, Tree};

fn main() {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!("usage: retention <corpus-dir> <A|B|C>");
        std::process::exit(2);
    }));
    let variant = args.next().unwrap_or_else(|| "A".to_string());

    let registry = ParserRegistry::new().expect("registry");
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();

    let mut parsers: Vec<(LanguageId, Parser)> = Vec::new();
    let mut trees: Vec<Tree> = Vec::new();
    let mut sources: Vec<Vec<u8>> = Vec::new();
    let mut facts = 0usize;
    let mut bytes = 0usize;

    for path in &files {
        let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(language) = LanguageId::from_extension(extension) else {
            continue;
        };
        let Ok(source) = std::fs::read(path) else {
            continue;
        };
        let adapter = registry.adapter(language);
        let parser = match parsers.iter_mut().find(|(l, _)| *l == language) {
            Some((_, parser)) => parser,
            None => {
                let mut parser = Parser::new();
                parser
                    .set_language(&adapter.ts_language())
                    .expect("grammar must load");
                parsers.push((language, parser));
                &mut parsers.last_mut().expect("just pushed").1
            }
        };
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        let analysis = extract_from_tree(adapter, name, &source, &tree);
        facts += analysis.declarations.len()
            + analysis.imports.len()
            + analysis.references.len()
            + analysis.calls.len()
            + analysis.file_test_evidence.len();
        bytes += source.len();
        std::hint::black_box(&analysis);

        match variant.as_str() {
            "A" => {
                drop(tree);
                drop(source);
            }
            "B" => {
                trees.push(tree);
                drop(source);
            }
            "C" => {
                trees.push(tree);
                sources.push(source);
            }
            other => {
                eprintln!("unknown variant {other:?}; expected A, B or C");
                std::process::exit(2);
            }
        }
    }

    let peak = peak_rss_kib().unwrap_or(0);
    println!(
        "variant={variant} files={} bytes={} facts={} retained_trees={} retained_sources={} peak_rss_kib={peak}",
        files.len(),
        bytes,
        facts,
        trees.len(),
        sources.len(),
    );
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) != Some(".git") {
                collect(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}

/// Kernel high-water mark for this process, in KiB, from `/proc/self/status`.
fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            return rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok());
        }
    }
    None
}
