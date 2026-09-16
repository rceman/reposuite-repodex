//! Range and line-index tests.
//!
//! `LineIndex` replaced an `O(offset)` scan that made extraction quadratic in
//! the file size. These tests pin its semantics against an independent
//! reference implementation so the optimisation cannot silently change what a
//! range means.

mod support;

use repodex::model::{LineIndex, SourceRange};

/// Independent reference: count newlines up to the offset and subtract.
fn reference_point_at(source: &[u8], offset: u32) -> (u32, u32) {
    let offset = (offset as usize).min(source.len());
    let mut row = 0u32;
    let mut line_start = 0usize;
    for (index, byte) in source.iter().enumerate().take(offset) {
        if *byte == b'\n' {
            row += 1;
            line_start = index + 1;
        }
    }
    (row, (offset - line_start) as u32)
}

fn check(source: &[u8]) {
    let index = LineIndex::new(source);
    for offset in 0..=(source.len() as u32 + 4) {
        assert_eq!(
            index.point_at(offset),
            reference_point_at(source, offset),
            "offset {offset} of {:?}",
            String::from_utf8_lossy(source)
        );
    }
    let full = index.range(0, source.len() as u32);
    assert_eq!(full.byte_start, 0);
    assert_eq!(full.byte_end, source.len() as u32);
    assert_eq!((full.row_start, full.column_start), (0, 0));
    assert_eq!(
        (full.row_end, full.column_end),
        reference_point_at(source, source.len() as u32)
    );
}

#[test]
fn line_index_matches_the_reference_for_many_shapes() {
    check(b"");
    check(b"\n");
    check(b"\n\n\n");
    check(b"a");
    check(b"a\n");
    check(b"a\nb");
    check(b"a\nb\n");
    check(b"a\r\nb\r\n");
    check(b"\r\n\r\n");
    check("pub fn a() {}\npub fn b() {}\n".as_bytes());
    check("\u{3b1}\u{3b2}\u{3b3}\n\u{65e5}\u{672c}\u{8a9e}\n".as_bytes());
    check(b"mixed\r\nline\nendings\r\n");
    check(b"no final newline at all");
}

#[test]
fn line_index_handles_a_large_source_linearly() {
    let mut source = Vec::with_capacity(256 * 1024);
    while source.len() < 256 * 1024 {
        source.extend_from_slice(b"fn filler() {}\n");
    }
    let index = LineIndex::new(&source);
    let start = std::time::Instant::now();
    for _ in 0..10_000 {
        // 10_000 lookups in a 256 KiB file must not scan the file each time.
        std::hint::black_box(index.point_at(source.len() as u32 / 2));
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 500,
        "10_000 line-index lookups took {elapsed:?}; the index must not scan the source per lookup"
    );
}

#[test]
fn ranges_are_half_open_and_columns_are_utf8_bytes() {
    let source = "let x = 1;\nlet \u{3b1}\u{3b2} = 2;\n";
    let index = LineIndex::new(source.as_bytes());
    let start = source.find("let \u{3b1}").expect("second line") as u32;
    let end = start + "let \u{3b1}\u{3b2}".len() as u32;
    let range = index.range(start, end);
    assert_eq!(&source[start as usize..end as usize], "let \u{3b1}\u{3b2}");
    assert_eq!(range.byte_len(), 8);
    assert_eq!(range.row_start, 1);
    assert_eq!(range.column_start, 0);
    assert_eq!(range.row_end, 1);
    // Eight bytes on one line: columns are byte columns, not character columns.
    assert_eq!(range.column_end, 8);
    assert_eq!("let \u{3b1}\u{3b2}".chars().count(), 6);
    assert!(!range.is_empty());
    assert!(range.contains(&range));
}

#[test]
fn empty_ranges_are_zero_width_but_well_placed() {
    let source = "abc\ndef\n";
    let index = LineIndex::new(source.as_bytes());
    let range = index.range(4, 4);
    assert!(range.is_empty());
    assert_eq!(range.byte_len(), 0);
    assert_eq!((range.row_start, range.column_start), (1, 0));
    assert_eq!((range.row_end, range.column_end), (1, 0));
}

#[test]
fn range_helpers_agree_with_the_index() {
    let source = "one\ntwo\nthree\n";
    let index = LineIndex::new(source.as_bytes());
    for (start, end) in [(0u32, 3u32), (4, 7), (8, 13), (0, 14)] {
        let from_index = index.range(start, end);
        let from_helper = repodex::model::range_from_offsets(source.as_bytes(), start, end);
        assert_eq!(from_index, from_helper);
        assert_eq!(
            repodex::model::point_at(source.as_bytes(), start),
            index.point_at(start)
        );
    }
}

#[test]
fn derived_ranges_in_real_extractions_agree_with_the_source() {
    // Declaration headers are derived ranges: they must select real source text.
    for (relative, header_kinds) in [
        (
            "rust/declarations.rs",
            vec!["struct", "enum", "trait", "module"],
        ),
        ("go/declarations.go", vec!["struct", "interface"]),
        (
            "php/declarations.php",
            vec!["class", "interface", "trait", "enum"],
        ),
    ] {
        let analysis = support::analyze_fixture(relative);
        let bytes = support::fixture_source(relative);
        let text = String::from_utf8(bytes).expect("utf-8");
        for scope in &analysis.scopes {
            let Some(header) = scope.header_range else {
                continue;
            };
            if !header_kinds.contains(&scope.kind.as_str()) {
                continue;
            }
            assert!(header.byte_start < header.byte_end, "{relative}");
            let slice = &text[header.byte_start as usize..header.byte_end as usize];
            assert!(
                !slice.contains('{'),
                "{relative}: a header range must stop before the body, got {slice:?}"
            );
            assert!(
                slice.contains(scope.name.as_deref().unwrap_or("")),
                "{relative}: header {slice:?} must contain the name"
            );
            assert_eq!(
                header.row_start,
                SourceRange::new(
                    header.byte_start,
                    header.byte_end,
                    header.row_start,
                    header.column_start,
                    header.row_end,
                    header.column_end
                )
                .row_start
            );
        }
    }
}
