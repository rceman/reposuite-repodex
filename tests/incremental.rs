//! Incremental parsing equivalence tests.
//!
//! Every case compares an incrementally parsed tree against an independent full
//! parse of the same bytes, using complete canonical facts plus tree structure,
//! ranges, diagnostics and recovery state. `changed_ranges` is recorded as
//! observation only.

mod support;

use repodex::incremental::{run_case, run_sequence, TextEdit};
use repodex::parser::{Analyzer, AnalyzerConfig, ParserRegistry};
use repodex::LanguageId;
use tree_sitter::Parser;

/// A registry plus a parser bound to one language. The registry owns the
/// adapter, so it must outlive the parser.
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

    /// Run one case against this harness.
    fn run(
        &mut self,
        path: &str,
        source: &str,
        edit: &TextEdit,
    ) -> repodex::incremental::IncrementalOutcome {
        let adapter = self.registry.adapter(self.language);
        run_case(adapter, &mut self.parser, path, source, edit).expect("case must run")
    }

    /// Run a chain of edits against this harness.
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

const RUST_SOURCE: &str = r#"use std::collections::HashMap;

pub struct Widget {
    pub name: String,
    count: u32,
}

impl Widget {
    pub fn compute(&self, value: u32) -> u32 {
        value + self.count
    }
}

pub fn helper(input: u32) -> u32 {
    let map = HashMap::new();
    let widget = Widget { name: String::from("x"), count: input };
    widget.compute(input)
}

#[test]
fn test_helper() {}
"#;

const GO_SOURCE: &str = r#"package sample

import (
	"fmt"
	"strings"
)

type Widget struct {
	Name  string
	Count int
}

func (w Widget) Compute(value int) int {
	return value + w.Count
}

func helper(input int) int {
	message := fmt.Sprintf("%d", input)
	return len(strings.TrimSpace(message))
}

func TestHelper(t *testing.T) {
	helper(1)
}
"#;

const PYTHON_SOURCE: &str = r#"import os
from typing import Final

LIMIT: Final = 10


class Widget:
    def compute(self, value):
        return value + LIMIT


def helper(input_value):
    widget = Widget()
    return widget.compute(input_value)


def test_helper():
    helper(1)
"#;

const PHP_SOURCE: &str = r#"<?php

namespace App\Sample;

use App\Support\Logger;

final class Widget
{
    public function compute(int $value): int
    {
        return $value + 1;
    }
}

function helper(int $input): int
{
    $widget = new Widget();
    return $widget->compute($input);
}
"#;

fn assert_equivalent(label: &str, outcome: &repodex::incremental::IncrementalOutcome) {
    assert!(
        outcome.equivalent(),
        "{label}: incremental and full parse diverged: {:?}",
        outcome.details
    );
    assert_eq!(
        outcome.incremental.status, outcome.full.status,
        "{label}: status diverged"
    );
}

/// Every required Rust and Go incremental case.
fn required_cases(
    language: LanguageId,
    source: &str,
    path: &str,
    offset_of: impl Fn(&str) -> usize,
) {
    let mut harness = Harness::new(language);
    let cases: Vec<(&str, TextEdit)> = vec![
        // Same-length identifier rename.
        (
            "same-length rename",
            TextEdit::replace(offset_of("helper"), offset_of("helper") + 6, "helpor"),
        ),
        // Insertion.
        (
            "insertion",
            TextEdit::insert(offset_of("pub fn helper"), "// inserted comment\n"),
        ),
        // Deletion.
        (
            "deletion",
            TextEdit::delete(offset_of("pub fn helper"), offset_of("pub fn helper") + 1),
        ),
        // Newline insertion.
        (
            "newline insertion",
            TextEdit::insert(offset_of("helper"), "\n"),
        ),
        // UTF-8 sensitive edit: replace ASCII with multibyte text of a
        // different byte length.
        (
            "utf-8 multibyte insertion",
            TextEdit::insert(
                offset_of("helper"),
                "// \u{3b1}\u{3b2}\u{3b3} \u{65e5}\u{672c}\u{8a9e}\n",
            ),
        ),
        (
            "utf-8 multibyte replacement",
            TextEdit::replace(
                offset_of("helper"),
                offset_of("helper") + 6,
                "\u{3b1}\u{3b2}\u{3b3}",
            ),
        ),
        // Edit near EOF.
        (
            "edit near eof",
            TextEdit::insert(source.len(), "\n// trailing\n"),
        ),
    ];
    for (label, edit) in cases {
        let outcome = harness.run(path, source, &edit);
        assert_equivalent(&format!("{path} {label}"), &outcome);
    }
}

#[test]
fn rust_required_incremental_cases() {
    let offset = |needle: &str| {
        RUST_SOURCE
            .find(needle)
            .unwrap_or_else(|| panic!("fixture must contain `{needle}`"))
    };
    required_cases(LanguageId::Rust, RUST_SOURCE, "sample.rs", offset);
}

#[test]
fn go_required_incremental_cases() {
    let offset = |needle: &str| {
        GO_SOURCE
            .find(needle)
            .unwrap_or_else(|| panic!("fixture must contain `{needle}`"))
    };
    // `helper` appears as `func helper`, so anchor on a unique substring.
    let cases: Vec<(&str, TextEdit)> = vec![
        (
            "same-length rename",
            TextEdit::replace(
                GO_SOURCE.find("func helper").expect("helper") + 5,
                GO_SOURCE.find("func helper").expect("helper") + 11,
                "helpor",
            ),
        ),
        (
            "insertion",
            TextEdit::insert(
                GO_SOURCE.find("func helper").expect("helper"),
                "// inserted\n",
            ),
        ),
        (
            "deletion",
            TextEdit::delete(
                GO_SOURCE.find("func helper").expect("helper"),
                GO_SOURCE.find("func helper").expect("helper") + 5,
            ),
        ),
        (
            "newline insertion",
            TextEdit::insert(GO_SOURCE.find("func helper").expect("helper"), "\n"),
        ),
        (
            "utf-8 multibyte insertion",
            TextEdit::insert(
                GO_SOURCE.find("func helper").expect("helper"),
                "// \u{3b1}\u{3b2}\u{3b3}\n",
            ),
        ),
        (
            "edit near eof",
            TextEdit::insert(GO_SOURCE.len(), "\n// trailing\n"),
        ),
    ];
    let mut harness = Harness::new(LanguageId::Go);
    for (label, edit) in cases {
        let outcome = harness.run("sample.go", GO_SOURCE, &edit);
        assert_equivalent(&format!("go {label}"), &outcome);
    }
    let _ = offset;
}

#[test]
fn introducing_and_repairing_a_syntax_error_converges() {
    let start = RUST_SOURCE.find("pub fn helper").expect("helper");
    let mut harness = Harness::new(LanguageId::Rust);
    let edits = vec![
        // Break the function by removing its parameter list opener.
        TextEdit::replace(start + 13, start + 14, ""),
        // Repair it.
        TextEdit::insert(start + 13, "("),
    ];
    let outcomes = harness.run_sequence("sample.rs", RUST_SOURCE, &edits);
    assert_eq!(outcomes.len(), 2);
    assert_equivalent("rust break", &outcomes[0]);
    assert_equivalent("rust repair", &outcomes[1]);
    // The damaged intermediate step really is damaged.
    assert_eq!(
        outcomes[0].incremental.status,
        repodex::AnalysisStatus::Recovered
    );
    // The repaired step is clean again.
    assert_eq!(
        outcomes[1].incremental.status,
        repodex::AnalysisStatus::Clean
    );
}

#[test]
fn multiple_consecutive_edits_stay_equivalent() {
    let mut harness = Harness::new(LanguageId::Go);
    let helper = GO_SOURCE.find("func helper").expect("helper");
    let edits = vec![
        TextEdit::insert(helper, "// first\n"),
        TextEdit::insert(helper, "// second\n"),
        TextEdit::replace(helper, helper + 5, "// comment"),
        TextEdit::delete(helper, helper + 12),
        TextEdit::insert(GO_SOURCE.len(), "\n// tail\n"),
    ];
    let outcomes = harness.run_sequence("sample.go", GO_SOURCE, &edits);
    for (index, outcome) in outcomes.iter().enumerate() {
        assert_equivalent(&format!("go chained edit {index}"), outcome);
        // Changed ranges are recorded, never used as the invalidation mechanism.
        assert!(
            outcome
                .changed_ranges
                .iter()
                .all(|range| range.byte_start <= range.byte_end),
            "changed ranges must be well ordered"
        );
    }
}

#[test]
fn crlf_sensitive_edit_stays_equivalent() {
    let crlf_source = RUST_SOURCE.replace('\n', "\r\n");
    let mut harness = Harness::new(LanguageId::Rust);
    let helper = crlf_source.find("pub fn helper").expect("helper");
    let edits = vec![
        TextEdit::insert(helper, "// inserted\r\n"),
        TextEdit::replace(helper, helper + 12, "pub fn helpr"),
        TextEdit::insert(crlf_source.len(), "\r\n// tail\r\n"),
    ];
    let outcomes = harness.run_sequence("sample.rs", &crlf_source, &edits);
    for (index, outcome) in outcomes.iter().enumerate() {
        assert_equivalent(&format!("crlf edit {index}"), outcome);
    }
}

/// Go requires the same CRLF coverage as Rust. The point of the case is that an
/// edit whose new text contains `\r\n` must produce a tree and a normalized
/// analysis identical to a fresh parse of the same bytes: the row/column points
/// of every node shift, so a comparator that ignored points would not notice a
/// divergence.
#[test]
fn go_crlf_sensitive_edit_stays_equivalent() {
    let crlf_source = GO_SOURCE.replace('\n', "\r\n");
    assert!(crlf_source.contains("\r\n"));
    let mut harness = Harness::new(LanguageId::Go);
    let helper = crlf_source.find("func helper").expect("helper");
    let test_helper = crlf_source.find("func TestHelper").expect("TestHelper");
    // Edits are applied in descending offset order, so every offset computed
    // against the original source stays valid for the whole sequence.
    let edits = vec![
        // Append a CRLF-terminated declaration at EOF.
        TextEdit::insert(
            crlf_source.len(),
            "\r\nfunc tail() int {\r\n\treturn 1\r\n}\r\n",
        ),
        // Replace one CRLF with a bare LF, so the file ends up with mixed line
        // endings. Row/column points shift for everything after this point.
        TextEdit::replace(test_helper - 2, test_helper, "\n"),
        // Same-length identifier rename.
        TextEdit::replace(helper + 5, helper + 11, "helprr"),
        // Insert a CRLF-terminated comment before the function.
        TextEdit::insert(helper, "// inserted\r\n"),
    ];
    let outcomes = harness.run_sequence("sample.go", &crlf_source, &edits);
    assert_eq!(outcomes.len(), 4);
    for (index, outcome) in outcomes.iter().enumerate() {
        assert_equivalent(&format!("go crlf edit {index}"), outcome);
        // The comparator must actually be looking at points, not only bytes.
        assert!(
            outcome.tree_structure_equal,
            "go crlf edit {index}: trees must agree on kinds, bytes, points, \
             namedness, field names and recovery state"
        );
    }
    // The final state is a real Go file with the appended function, and it is
    // clean: none of the edits damaged the syntax.
    let final_analysis = &outcomes[3].incremental;
    assert!(final_analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "tail"));
    assert!(final_analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "helprr"));
    assert_eq!(final_analysis.status, repodex::AnalysisStatus::Clean);
    assert!(final_analysis.recovery_regions.is_empty());
}

/// Go requires the same introduce-error → repair coverage as Rust. The damaged
/// and repaired states are asserted explicitly; this is not just an arbitrary
/// multi-edit chain.
#[test]
fn go_introducing_and_repairing_a_syntax_error_converges() {
    let start = GO_SOURCE.find("func helper").expect("helper");
    let mut harness = Harness::new(LanguageId::Go);
    // `func helper(input int) int {` -> drop the opening brace, leaving the
    // function header without a body.
    let brace = GO_SOURCE[start..].find('{').expect("brace") + start;
    let edits = vec![
        TextEdit::replace(brace, brace + 1, ""),
        TextEdit::insert(brace, "{"),
    ];
    let outcomes = harness.run_sequence("sample.go", GO_SOURCE, &edits);
    assert_eq!(outcomes.len(), 2);

    // Damaged state.
    assert_equivalent("go break", &outcomes[0]);
    assert_eq!(
        outcomes[0].incremental.status,
        repodex::AnalysisStatus::Recovered,
        "removing the body brace must produce a recovered parse"
    );
    assert!(
        !outcomes[0].incremental.recovery_regions.is_empty(),
        "the damaged step must report at least one recovery region"
    );
    assert!(
        outcomes[0].incremental.error_count() > 0,
        "the damaged step must report at least one error diagnostic"
    );

    // Repaired state.
    assert_equivalent("go repair", &outcomes[1]);
    assert_eq!(
        outcomes[1].incremental.status,
        repodex::AnalysisStatus::Clean,
        "restoring the brace must return to a clean parse"
    );
    assert!(
        outcomes[1].incremental.recovery_regions.is_empty(),
        "the repaired step must report no recovery regions"
    );
    assert_eq!(outcomes[1].incremental.error_count(), 0);

    // The repaired analysis reproduces a fresh parse of the same bytes exactly,
    // and `helper` is a declaration again.
    let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
    let fresh = analyzer.analyze_bytes("sample.go", LanguageId::Go, GO_SOURCE.as_bytes());
    assert!(outcomes[1].incremental.same_facts(&fresh));
    assert!(fresh
        .declarations
        .iter()
        .any(|declaration| declaration.name == "helper"));
}

#[test]
fn python_incremental_case() {
    let mut harness = Harness::new(LanguageId::Python);
    let helper = PYTHON_SOURCE.find("def helper").expect("helper");
    let edits = vec![
        TextEdit::replace(helper + 4, helper + 10, "helpor"),
        TextEdit::insert(helper, "@decorator\n"),
        TextEdit::insert(helper, "def renamed_again():\n    pass\n\n"),
        TextEdit::insert(helper, "# \u{3b1}\u{3b2}\u{3b3}\n"),
    ];
    let outcomes = harness.run_sequence("sample.py", PYTHON_SOURCE, &edits);
    for (index, outcome) in outcomes.iter().enumerate() {
        assert_equivalent(&format!("python edit {index}"), outcome);
    }
    assert_eq!(outcomes.len(), 4);
}

#[test]
fn php_incremental_case() {
    let mut harness = Harness::new(LanguageId::Php);
    let helper = PHP_SOURCE.find("function helper").expect("helper");
    let edits = vec![
        TextEdit::replace(helper + 9, helper + 15, "helpor"),
        TextEdit::insert(helper, "// inserted\n"),
        TextEdit::insert(helper, "function added(): void\n{\n}\n\n"),
        TextEdit::insert(PHP_SOURCE.len(), "\n// tail\n"),
    ];
    let outcomes = harness.run_sequence("sample.php", PHP_SOURCE, &edits);
    for (index, outcome) in outcomes.iter().enumerate() {
        assert_equivalent(&format!("php edit {index}"), outcome);
    }
    assert_eq!(outcomes.len(), 4);
}

#[test]
fn incremental_extraction_matches_full_extraction_on_real_fixtures() {
    let mut harness = Harness::new(LanguageId::Rust);
    let source = std::fs::read_to_string(support::fixture("rust/impls.rs")).expect("fixture");
    // Rename `Gadget` to `Gadgetx` (same length) and back.
    let gadget = source.find("Gadget").expect("Gadget");
    let edits = vec![
        TextEdit::replace(gadget, gadget + 6, "Gadgtx"),
        TextEdit::replace(gadget, gadget + 6, "Gadget"),
    ];
    let outcomes = harness.run_sequence("impls.rs", &source, &edits);
    assert_equivalent("fixture rename", &outcomes[0]);
    assert_equivalent("fixture restore", &outcomes[1]);
    // The restored step must reproduce the original facts exactly.
    let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
    let original = analyzer.analyze_bytes("impls.rs", LanguageId::Rust, source.as_bytes());
    assert_eq!(
        outcomes[1].incremental.canonical_text(),
        original.canonical_text()
    );
}
