//! Repository scanner tests.

mod support;

use repodex::scanner::{FileOutcome, ScanOptions, Scanner, PRUNED_DIRECTORIES};
use support::{analyzer, TempDir};

fn scan(root: &std::path::Path) -> repodex::scanner::ScanReport {
    let analyzer = analyzer();
    Scanner::new(&analyzer, ScanOptions::default())
        .scan(root)
        .expect("scan must succeed")
}

#[test]
fn discovery_is_deterministic_and_sorted() {
    let temp = TempDir::new("scanner-order");
    // Create files in an order unrelated to their sorted names.
    for name in ["zeta.rs", "alpha.rs", "middle.rs"] {
        temp.write(name, b"fn f() {}\n");
    }
    let first = scan(temp.path());
    let second = scan(temp.path());
    let paths = first
        .files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec!["alpha.rs", "middle.rs", "zeta.rs"]);
    assert_eq!(
        paths,
        second
            .files
            .iter()
            .map(|f| f.relative_path.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        repodex::canonical::scan_digest(&first),
        repodex::canonical::scan_digest(&second)
    );
}

#[test]
fn pruned_directories_are_never_visited() {
    let temp = TempDir::new("scanner-prune");
    temp.write("src/main.rs", b"fn main() {}\n");
    for directory in PRUNED_DIRECTORIES {
        temp.write(&format!("{directory}/hidden.rs"), b"fn hidden() {}\n");
    }
    let report = scan(temp.path());
    assert_eq!(report.visited_files, 1);
    assert_eq!(report.supported_files, 1);
    assert!(report
        .files
        .iter()
        .all(|file| file.relative_path == "src/main.rs"));
}

#[test]
fn unsupported_extensions_are_counted_but_not_parsed() {
    let temp = TempDir::new("scanner-unsupported");
    temp.write("src/main.rs", b"fn main() {}\n");
    temp.write("README.md", b"# readme\n");
    temp.write("data.json", b"{}\n");
    let report = scan(temp.path());
    assert_eq!(report.visited_files, 3);
    assert_eq!(report.supported_files, 1);
    assert_eq!(report.unsupported_files, 2);
    assert_eq!(report.trees_returned(), 1);
    assert!(report
        .files
        .iter()
        .filter(|file| file.outcome == FileOutcome::Unsupported)
        .all(|file| file.analysis.is_none()));
}

#[test]
fn all_four_languages_are_dispatched() {
    let temp = TempDir::new("scanner-languages");
    temp.write("a.rs", b"pub fn rust_fn() {}\n");
    temp.write("b.go", b"package p\n\nfunc GoFunc() {}\n");
    temp.write("c.py", b"def python_fn():\n    pass\n");
    temp.write("d.php", b"<?php\nfunction php_fn() {}\n");
    let report = scan(temp.path());
    assert_eq!(report.supported_files, 4);
    assert_eq!(report.parsed_clean_files, 4);
    // Go contributes two declarations (`package` and `func`), the others one.
    assert_eq!(report.declarations, 5);
    for file in &report.files {
        let analysis = file.analysis.as_ref().expect("analyzed");
        let expected = if file.relative_path == "b.go" { 2 } else { 1 };
        assert_eq!(
            analysis.declarations.len(),
            expected,
            "{}",
            file.relative_path
        );
    }
}

#[test]
fn one_bad_file_does_not_abort_the_scan() {
    let temp = TempDir::new("scanner-bad-file");
    temp.write("good.rs", b"pub fn good() {}\n");
    temp.write("invalid_utf8.rs", &[0x66, 0x6e, 0x20, 0xff, 0xfe, 0x0a]);
    temp.write("recovered.rs", b"fn broken( {\n}\n");
    let report = scan(temp.path());
    assert_eq!(report.supported_files, 3);
    assert_eq!(report.parsed_clean_files, 1);
    assert_eq!(report.parsed_with_recovery_files, 1);
    assert_eq!(report.encoding_skips, 1);
    assert_eq!(report.skipped_files(), 1);
    // The scan still produced facts for the readable files.
    assert_eq!(report.declarations, 2);
    let invalid = report
        .files
        .iter()
        .find(|file| file.relative_path == "invalid_utf8.rs")
        .expect("invalid file");
    assert_eq!(invalid.outcome, FileOutcome::Skipped);
    assert!(invalid
        .analysis
        .as_ref()
        .expect("analysis")
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.kind == repodex::DiagnosticKind::UnsupportedEncoding));
}

#[test]
fn oversized_files_are_skipped_unless_the_limit_is_raised() {
    let temp = TempDir::new("scanner-oversized");
    let mut body = String::from("pub fn big() {}\n");
    while body.len() < 4096 {
        body.push_str("pub fn filler() {}\n");
    }
    temp.write("big.rs", body.as_bytes());

    let analyzer = analyzer();
    let strict = Scanner::new(
        &analyzer,
        ScanOptions {
            respect_gitignore: true,
        },
    );
    let report = strict.scan(temp.path()).expect("scan");
    // The default 8 MiB limit is not hit, so this is a control run.
    assert_eq!(report.parsed_clean_files, 1);

    let small_limit = repodex::parser::AnalyzerConfig { max_file_size: 64 };
    let analyzer = repodex::parser::Analyzer::new(small_limit).expect("analyzer");
    let strict = Scanner::new(&analyzer, ScanOptions::default());
    let report = strict.scan(temp.path()).expect("scan");
    assert_eq!(report.size_limit_skips, 1);
    assert_eq!(report.parsed_clean_files, 0);
    assert_eq!(report.bytes_processed, 0);
}

#[test]
fn symlinks_are_not_followed() {
    let temp = TempDir::new("scanner-symlink");
    temp.write("real/target.rs", b"pub fn target() {}\n");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(temp.path().join("real"), temp.path().join("link_dir"))
            .expect("symlink dir");
        std::os::unix::fs::symlink(
            temp.path().join("real/target.rs"),
            temp.path().join("link_file.rs"),
        )
        .expect("symlink file");
    }
    let report = scan(temp.path());
    #[cfg(unix)]
    {
        // Neither the linked directory nor the linked file is followed.
        assert_eq!(report.supported_files, 1);
        assert_eq!(report.files[0].relative_path, "real/target.rs");
    }
    #[cfg(not(unix))]
    {
        assert_eq!(report.supported_files, 1);
    }
}

#[test]
fn gitignore_is_optional_and_root_scoped() {
    let temp = TempDir::new("scanner-gitignore");
    temp.write(".gitignore", b"ignored.rs\n");
    temp.write("ignored.rs", b"pub fn ignored() {}\n");
    temp.write("kept.rs", b"pub fn kept() {}\n");
    // `ignore` only applies gitignore rules inside a repository.
    temp.write(".git/HEAD", b"ref: refs/heads/main\n");

    let analyzer = analyzer();
    let respecting = Scanner::new(&analyzer, ScanOptions::default());
    let report = respecting.scan(temp.path()).expect("scan");
    let names = report
        .files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();
    assert!(names.contains(&"kept.rs".to_string()));
    assert!(!names.contains(&"ignored.rs".to_string()));

    let ignoring = Scanner::new(
        &analyzer,
        ScanOptions {
            respect_gitignore: false,
        },
    );
    let report = ignoring.scan(temp.path()).expect("scan");
    let names = report
        .files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();
    assert!(names.contains(&"ignored.rs".to_string()));
}

#[test]
fn counters_are_internally_consistent() {
    let temp = TempDir::new("scanner-counters");
    temp.write("clean.rs", b"pub fn clean() {}\npub fn clean_two() {}\n");
    temp.write("broken.go", b"package p\n\nfunc broken( {\n}\n");
    temp.write("notes.txt", b"hello\n");
    let report = scan(temp.path());
    assert_eq!(
        report.visited_files,
        report.supported_files + report.unsupported_files
    );
    assert_eq!(
        report.trees_returned(),
        report.parsed_clean_files + report.parsed_with_recovery_files
    );
    assert_eq!(
        report.supported_files,
        report.parsed_clean_files
            + report.parsed_with_recovery_files
            + report.skipped_files()
            + report.failed_files()
    );
    // clean.rs declares two functions; broken.go declares `package` and a
    // recovered `func broken`.
    assert_eq!(report.declarations, 4);
    let expected_bytes = std::fs::metadata(temp.path().join("clean.rs"))
        .expect("metadata")
        .len()
        + std::fs::metadata(temp.path().join("broken.go"))
            .expect("metadata")
            .len();
    assert_eq!(report.bytes_processed, expected_bytes);
    assert!(report.bytes_visited >= report.bytes_processed);
    let rate = report.recovery_rate_among_parsed().expect("rate");
    assert!((rate - 0.5).abs() < f64::EPSILON);
}

#[test]
fn scanning_a_missing_directory_is_an_error_not_a_panic() {
    let analyzer = analyzer();
    let scanner = Scanner::new(&analyzer, ScanOptions::default());
    let missing = std::env::temp_dir().join("repodex-test-does-not-exist-42");
    let _ = std::fs::remove_dir_all(&missing);
    assert!(scanner.scan(&missing).is_err());
}
