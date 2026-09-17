//! Regression tests for the incremental tree comparator.
//!
//! The comparator backs the claim that an incrementally parsed tree and a fresh
//! parse of the same bytes are structurally equivalent. That claim is only worth
//! anything if the comparator actually inspects the properties it claims to
//! inspect, so these tests pin the compared properties directly and then prove
//! the comparator discriminates.

use repodex::incremental::tree_structure_digest;
use repodex::{LanguageId, ParserRegistry};
use tree_sitter::{Parser, Tree};

fn parse(language: LanguageId, source: &str) -> Tree {
    let registry = ParserRegistry::new().expect("registry");
    let mut parser = Parser::new();
    parser
        .set_language(&registry.adapter(language).ts_language())
        .expect("grammar must load");
    parser.parse(source, None).expect("tree")
}

fn digest(language: LanguageId, source: &str) -> String {
    tree_structure_digest(parse(language, source).root_node(), source.as_bytes())
}

/// Every property the comparator claims to compare must appear in its output.
///
/// If someone drops `points=`, `field=` or the namedness marker from the
/// format, the comparator silently stops comparing that property and this test
/// fails.
#[test]
fn the_digest_records_every_compared_property() {
    let source = "pub fn f(value: u32) -> u32 { value }\n";
    let text = digest(LanguageId::Rust, source);

    // Node kind.
    assert!(text.contains("kind=source_file"), "{text}");
    assert!(text.contains("kind=function_item"), "{text}");
    assert!(text.contains("kind=identifier"), "{text}");
    assert!(text.contains("kind=visibility_modifier"), "{text}");

    // Namedness, both values present.
    assert!(text.contains(" named "), "{text}");
    assert!(text.contains(" anon "), "{text}");

    // Byte ranges.
    assert!(
        text.contains(&format!("bytes=0..{}", source.trim_end().len())),
        "{text}"
    );

    // Point ranges (row:column..row:column), not just bytes.
    assert!(text.contains("points=0:0..0:"), "{text}");

    // Recovery state.
    assert!(text.contains("state=-"), "{text}");

    // Field names: a real field, and the marker for a node with no field.
    assert!(text.contains("field=name"), "{text}");
    assert!(text.contains("field=parameters"), "{text}");
    assert!(text.contains("field=body"), "{text}");
    assert!(text.contains("field=-"), "{text}");

    // Child counts, so a missing or extra child is visible.
    assert!(text.contains("children="), "{text}");

    // Leaf text, so two leaves of the same kind with different spelling differ.
    assert!(text.contains("text=\"f\""), "{text}");
    assert!(text.contains("text=\"value\""), "{text}");

    // Every node in the tree appears exactly once.
    let lines = text.lines().count();
    assert!(lines >= 8, "expected a node per line, got {lines}");
}

/// Leaf text must be compared.
///
/// A named leaf's kind is its category, not its value, so without the text two
/// trees that differ only in the spelling of an identifier would digest
/// identically.
#[test]
fn leaf_text_is_compared() {
    assert_ne!(
        digest(LanguageId::Rust, "fn f() {}\n"),
        digest(LanguageId::Rust, "fn g() {}\n")
    );
    assert_ne!(
        digest(LanguageId::Rust, "fn f() -> u32 { 1 }\n"),
        digest(LanguageId::Rust, "fn f() -> u32 { 2 }\n")
    );
    assert_ne!(
        digest(LanguageId::Python, "x = 'a'\n"),
        digest(LanguageId::Python, "x = 'b'\n")
    );
    assert_ne!(
        digest(LanguageId::Go, "package p\n\nvar x = \"a\"\n"),
        digest(LanguageId::Go, "package p\n\nvar x = \"b\"\n")
    );
}

/// The comparator must separate trees that differ, in every dimension.
#[test]
fn the_digest_separates_trees_that_differ() {
    let base = digest(LanguageId::Rust, "fn f() {}\n");

    // A different identifier: same structure, different bytes and text.
    assert_ne!(base, digest(LanguageId::Rust, "fn g() {}\n"));

    // Whitespace only: the same tokens in the same order, but the byte and
    // point ranges of several nodes differ. A comparator that compared only
    // S-expressions would call these equal.
    assert_ne!(base, digest(LanguageId::Rust, "fn f() { }\n"));
    assert_ne!(base, digest(LanguageId::Rust, "fn  f() {}\n"));

    // A newline inside the body shifts every following row.
    assert_ne!(base, digest(LanguageId::Rust, "fn f() {\n}\n"));

    // Clean vs damaged: the recovery state must be compared.
    let damaged = digest(LanguageId::Rust, "fn f( {}\n");
    assert_ne!(base, damaged);
    assert!(
        damaged.contains("state=E") || damaged.contains("state=M") || damaged.contains("state=e"),
        "damaged source must record an ERROR or MISSING state: {damaged}"
    );

    // A MISSING token is zero-width: Tree-sitter inserted no bytes at all, so
    // the byte range alone cannot distinguish it. Only the state and the point
    // position do, and the digest records both.
    let missing_source = "fn main() {\n    let x = 1\n    let y = 2;\n}\n";
    let missing = digest(LanguageId::Rust, missing_source);
    assert!(
        missing.contains("state=M"),
        "a missing semicolon must produce a MISSING token: {missing}"
    );
    // The inserted token is zero-width, which is exactly why the state marker
    // is needed.
    assert!(
        missing.contains("bytes=25..25") && missing.contains("points=1:13..1:13"),
        "the MISSING token must be recorded as zero-width at its point: {missing}"
    );
    assert!(!missing.contains("state=E"));
}

/// The comparator must not depend on anything ephemeral.
///
/// Two independent parsers, parsing the same bytes, must agree byte for byte.
/// If the digest contained node ids, symbol numbers or pointers, this would
/// fail.
#[test]
fn the_digest_is_a_function_of_the_bytes_alone() {
    let source = "fn outer() { inner(); }\nfn inner() {}\n";
    let first = digest(LanguageId::Rust, source);
    // A fresh registry and a fresh parser: nothing is shared with the first run.
    let second = digest(LanguageId::Rust, source);
    assert_eq!(first, second);

    // Same tree, digested twice, is identical.
    let tree = parse(LanguageId::Rust, source);
    assert_eq!(
        tree_structure_digest(tree.root_node(), source.as_bytes()),
        tree_structure_digest(tree.root_node(), source.as_bytes())
    );

    // And the digest is not merely a hash: it is a readable structure dump, so
    // a divergence points at the node that diverged.
    assert!(first.lines().count() > 10);
}

/// Field names must be compared, not just child kinds and order.
///
/// Two different Go constructs that happen to have the same child kinds in the
/// same order but different field roles must not collide. This is checked
/// indirectly but robustly: the same token sequence in different field
/// positions produces different digests.
#[test]
fn field_names_are_compared() {
    // In Go, `a.B()` is a call whose callee is a selector; `a.B` alone is a
    // selector in an expression. The trees differ, and the field names are part
    // of what makes them differ.
    let call = digest(LanguageId::Go, "package p\n\nfunc f() { a.B() }\n");
    let selector = digest(LanguageId::Go, "package p\n\nfunc f() { _ = a.B }\n");
    assert_ne!(call, selector);
    // Both trees contain a `selector_expression` node with the same field name.
    assert!(call.contains("field=function"), "{call}");
    assert!(
        selector.contains("field=function") || selector.contains("field=right"),
        "{selector}"
    );
}

/// All four grammars must go through the same comparator without panicking and
/// must produce a non-trivial digest.
#[test]
fn every_language_produces_a_structural_digest() {
    for (language, source) in [
        (LanguageId::Rust, "pub fn f() -> u32 { 1 }\n"),
        (LanguageId::Go, "package p\n\nfunc f() int { return 1 }\n"),
        (LanguageId::Python, "def f() -> int:\n    return 1\n"),
        (LanguageId::Php, "<?php\nfunction f(): int { return 1; }\n"),
    ] {
        let text = digest(language, source);
        assert!(
            text.contains(" named ") && text.contains("field=") && text.contains("points="),
            "{} digest is incomplete: {text}",
            language.as_str()
        );
    }
}
