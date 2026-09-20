//! Rust local-binding fact extraction tests.
//!
//! These tests encode the intended binding semantics directly — which name a
//! binding introduces, its kind, its written range, and the source regions it
//! can shadow — rather than depending on `fixtures/expected/**`. The visibility
//! assertions use `LocalBindingOccurrence::covers`, which is a pure source-range
//! test, never name resolution.

mod support;

use std::path::{Path, PathBuf};

use repodex::incremental::{run_case, run_sequence, TextEdit};
use repodex::parser::{Analyzer, AnalyzerConfig, ParserRegistry};
use repodex::repository::{
    build_snapshot, update_snapshot, BuildOptions, RepositoryFactIndex, RepositoryManifest,
};
use repodex::{BindingKind, FileAnalysis, LanguageId, LocalBindingOccurrence};
use support::TempDir;
use tree_sitter::Parser;

fn analyze(source: &str) -> FileAnalysis {
    Analyzer::new(AnalyzerConfig::default())
        .expect("analyzer")
        .analyze_bytes("probe.rs", LanguageId::Rust, source.as_bytes())
}

/// Byte start of the *n*th call occurrence whose callee text is `name`.
fn call_pos(analysis: &FileAnalysis, name: &str, n: usize) -> u32 {
    analysis
        .calls
        .iter()
        .filter(|call| call.callee_written == name)
        .nth(n)
        .unwrap_or_else(|| panic!("missing {n}th `{name}` call"))
        .callee_range
        .byte_start
}

fn bindings_named<'a>(
    analysis: &'a FileAnalysis,
    name: &'a str,
) -> Vec<&'a LocalBindingOccurrence> {
    analysis.bindings_named(name).collect()
}

fn only_binding<'a>(analysis: &'a FileAnalysis, name: &'a str) -> &'a LocalBindingOccurrence {
    let found = bindings_named(analysis, name);
    assert_eq!(found.len(), 1, "expected exactly one `{name}` binding");
    found[0]
}

fn source_of(source: &str, range: repodex::SourceRange) -> &str {
    &source[range.byte_start as usize..range.byte_end as usize]
}

// ---------------------------------------------------------------------------
// Binding kinds and pattern decomposition
// ---------------------------------------------------------------------------

#[test]
fn every_required_binding_kind_is_emitted() {
    let analysis = analyze(
        "fn f(p: u8) {\n\
         let a = 0;\n\
         let g = |c| c;\n\
         for d in it {}\n\
         match m { Some(e) => {}, _ => {} }\n\
         if let Ok(b) = r {}\n\
         while let Ok(h) = n() {}\n\
         }\n",
    );
    let kind_of = |name: &str| only_binding(&analysis, name).kind;
    assert_eq!(kind_of("a"), BindingKind::Let);
    assert_eq!(kind_of("p"), BindingKind::FunctionParameter);
    assert_eq!(kind_of("c"), BindingKind::ClosureParameter);
    assert_eq!(kind_of("d"), BindingKind::ForPattern);
    assert_eq!(kind_of("e"), BindingKind::MatchPattern);
    assert_eq!(kind_of("b"), BindingKind::IfLetPattern);
    assert_eq!(kind_of("h"), BindingKind::WhileLetPattern);
}

#[test]
fn pattern_decomposition_emits_each_bound_name() {
    let analysis = analyze(
        "fn f() {\n\
         let (left, right) = pair;\n\
         let Point { x, y } = point;\n\
         let S { f1: a, f2 } = s;\n\
         }\n",
    );
    for name in ["left", "right", "x", "y", "a", "f2"] {
        assert_eq!(
            only_binding(&analysis, name).kind,
            BindingKind::Let,
            "`{name}` must destructure into its own let binding"
        );
    }
    // The destructured field names `f1`/`f2` are not additional `f1` bindings —
    // `f1:` binds `a`, while `f2` shorthand binds `f2`.
    assert!(bindings_named(&analysis, "f1").is_empty());
}

#[test]
fn name_range_slices_exactly_to_the_bound_identifier() {
    let source = "fn f() { let (alpha, beta) = pair; }";
    let analysis = analyze(source);
    for binding in &analysis.bindings {
        assert_eq!(
            source_of(source, binding.name_range),
            binding.name,
            "name_range must slice exactly to the written identifier"
        );
        // The binding site must enclose the bound name.
        assert!(
            binding.binding_site_range.contains(&binding.name_range),
            "binding_site_range must contain the bound name"
        );
    }
}

#[test]
fn wildcards_and_non_name_positions_are_not_bindings() {
    let analysis = analyze(
        "fn f() {\n\
         let _ = f();\n\
         let _e = g();\n\
         let S { f: _ } = s;\n\
         }\n",
    );
    // `_` produces no binding at all; `_e` is a real (named) binding.
    assert!(analysis.bindings.iter().all(|b| b.name != "_"));
    assert_eq!(only_binding(&analysis, "_e").kind, BindingKind::Let);
}

#[test]
fn self_parameter_is_not_a_name_binding() {
    let analysis = analyze("impl S { fn m(&self, x: u8) {} fn n(mut self) {} }\n");
    assert!(
        analysis.bindings.iter().all(|b| b.name != "self"),
        "`self` is a receiver, not a callable name binding"
    );
    assert_eq!(
        only_binding(&analysis, "x").kind,
        BindingKind::FunctionParameter
    );
}

// ---------------------------------------------------------------------------
// `let` visibility (the mandatory acceptance cases)
// ---------------------------------------------------------------------------

#[test]
fn a_let_binding_does_not_cover_calls_before_it_or_its_own_initializer() {
    let analysis = analyze(
        "fn helper() {}\n\
         fn run() {\n\
         helper();\n\
         let helper = helper();\n\
         helper();\n\
         }\n",
    );
    let helper = only_binding(&analysis, "helper");
    assert_eq!(helper.kind, BindingKind::Let);
    // call 0: before the let — not covered.
    assert!(!helper.covers(call_pos(&analysis, "helper", 0)));
    // call 1: inside the let's own initializer — not covered.
    assert!(!helper.covers(call_pos(&analysis, "helper", 1)));
    // call 2: after the let — covered.
    assert!(helper.covers(call_pos(&analysis, "helper", 2)));
}

#[test]
fn a_nested_block_let_does_not_leak_outside_its_block() {
    let analysis = analyze(
        "fn helper() {}\n\
         fn run() {\n\
         {\n\
         let helper = || {};\n\
         helper();\n\
         }\n\
         helper();\n\
         }\n",
    );
    let helper = only_binding(&analysis, "helper");
    // call 0: inside the inner block — covered.
    assert!(helper.covers(call_pos(&analysis, "helper", 0)));
    // call 1: after the block closed — not covered.
    assert!(!helper.covers(call_pos(&analysis, "helper", 1)));
}

#[test]
fn a_let_visibility_stops_at_the_enclosing_block_end() {
    let analysis = analyze("fn f() {\nlet x = 0;\nx();\n}\nfn g() { x(); }\n");
    let x = only_binding(&analysis, "x");
    assert!(x.covers(call_pos(&analysis, "x", 0)));
    // The `x()` in `g` is outside `f`'s block — not covered.
    assert!(!x.covers(call_pos(&analysis, "x", 1)));
}

#[test]
fn sequential_same_name_lets_preserve_ordering_information() {
    let analysis = analyze("fn f() {\nlet x = a();\nx();\nlet x = b();\nx();\n}\n");
    let bindings = bindings_named(&analysis, "x");
    assert_eq!(bindings.len(), 2);
    // Both cover the tail, but the later binding starts later, so a consumer can
    // pick the nearest introduction for the last call.
    assert!(bindings[0].name_range.byte_start < bindings[1].name_range.byte_start);
    let second_call = call_pos(&analysis, "x", 1);
    assert!(bindings[1].covers(second_call));
    // The first binding's visibility also reaches the tail (overlapping is
    // allowed); the second is simply the nearer one.
    assert!(bindings[0].covers(second_call));
    // But only the first covers the call between the two lets.
    let first_call = call_pos(&analysis, "x", 0);
    assert!(bindings[0].covers(first_call));
    assert!(!bindings[1].covers(first_call));
}

// ---------------------------------------------------------------------------
// Parameters and closures
// ---------------------------------------------------------------------------

#[test]
fn a_function_parameter_covers_the_whole_body_but_not_outside() {
    let analysis = analyze("fn f(helper: fn()) {\nhelper();\n}\nfn g() { helper(); }\n");
    let helper = only_binding(&analysis, "helper");
    assert_eq!(helper.kind, BindingKind::FunctionParameter);
    assert!(helper.covers(call_pos(&analysis, "helper", 0)));
    assert!(!helper.covers(call_pos(&analysis, "helper", 1)));
}

#[test]
fn a_signature_only_parameter_binds_a_name_but_covers_nothing() {
    let analysis = analyze("trait T { fn f(x: u8); }\n");
    let x = only_binding(&analysis, "x");
    assert_eq!(x.kind, BindingKind::FunctionParameter);
    assert!(x.visibility_ranges.is_empty());
}

#[test]
fn a_closure_parameter_is_bounded_to_the_closure_body() {
    let analysis = analyze("fn f() {\nlet run = |helper| { helper(); };\nhelper();\n}\n");
    let helper = only_binding(&analysis, "helper");
    assert_eq!(helper.kind, BindingKind::ClosureParameter);
    // call 0: inside the closure body — covered.
    assert!(helper.covers(call_pos(&analysis, "helper", 0)));
    // call 1: in the enclosing block after the closure — not covered.
    assert!(!helper.covers(call_pos(&analysis, "helper", 1)));
}

#[test]
fn nested_closures_keep_their_parameters_bounded() {
    let analysis = analyze("fn f() {\nlet c = |p| { let d = |q| { q() }; };\n}\n");
    let p = only_binding(&analysis, "p");
    let q = only_binding(&analysis, "q");
    assert_eq!(p.kind, BindingKind::ClosureParameter);
    assert_eq!(q.kind, BindingKind::ClosureParameter);
    // `q`'s visibility is strictly inside `p`'s closure body.
    assert!(q.visibility_ranges[0].byte_start > p.visibility_ranges[0].byte_start);
    assert!(q.visibility_ranges[0].byte_end <= p.visibility_ranges[0].byte_end);
}

// ---------------------------------------------------------------------------
// for / match / if-let / while-let
// ---------------------------------------------------------------------------

#[test]
fn a_for_binding_covers_the_loop_body_only() {
    let analysis = analyze("fn f() {\nfor helper in it() {\nhelper();\n}\nhelper();\n}\n");
    let helper = only_binding(&analysis, "helper");
    assert_eq!(helper.kind, BindingKind::ForPattern);
    // call 0: the iterator `it()` — not covered.
    assert!(!helper.covers(call_pos(&analysis, "it", 0)));
    // call 1: inside the loop body — covered.
    assert!(helper.covers(call_pos(&analysis, "helper", 0)));
    // call 2: after the loop — not covered.
    assert!(!helper.covers(call_pos(&analysis, "helper", 1)));
}

#[test]
fn a_match_arm_binding_covers_guard_and_body_but_not_sibling_arms() {
    let analysis =
        analyze("fn f() {\nmatch m {\nSome(v) if v() => v(),\nSome(v) => v(),\n_ => {}\n}\n}\n");
    let bindings = bindings_named(&analysis, "v");
    assert_eq!(bindings.len(), 2);
    let arm0 = bindings[0];
    // Arm 0 `v` covers its guard `v()` and its body `v()` (two disjoint ranges).
    assert_eq!(arm0.visibility_ranges.len(), 2);
    assert!(arm0.covers(call_pos(&analysis, "v", 0)));
    assert!(arm0.covers(call_pos(&analysis, "v", 1)));
    // ...but not the sibling arm's `v()` (call index 2).
    assert!(!arm0.covers(call_pos(&analysis, "v", 2)));
    // Arm 1 `v` covers only its own body (no guard → one range).
    let arm1 = bindings[1];
    assert_eq!(arm1.visibility_ranges.len(), 1);
    assert!(arm1.covers(call_pos(&analysis, "v", 2)));
    assert!(!arm1.covers(call_pos(&analysis, "v", 0)));
}

#[test]
fn an_if_let_binding_covers_the_consequence_only() {
    let analysis = analyze(
        "fn f() {\nif let Some(helper) = src() {\nhelper();\n} else {\nhelper();\n}\nhelper();\n}\n",
    );
    let helper = only_binding(&analysis, "helper");
    assert_eq!(helper.kind, BindingKind::IfLetPattern);
    // call src() (the condition value) — not covered.
    assert!(!helper.covers(call_pos(&analysis, "src", 0)));
    // call 0 helper (then-branch) — covered.
    assert!(helper.covers(call_pos(&analysis, "helper", 0)));
    // call 1 helper (else-branch) — not covered.
    assert!(!helper.covers(call_pos(&analysis, "helper", 1)));
    // call 2 helper (after the whole if) — not covered.
    assert!(!helper.covers(call_pos(&analysis, "helper", 2)));
}

#[test]
fn a_while_let_binding_covers_the_loop_body_only() {
    let analysis =
        analyze("fn f() {\nwhile let Ok(helper) = next() {\nhelper();\n}\nhelper();\n}\n");
    let helper = only_binding(&analysis, "helper");
    assert_eq!(helper.kind, BindingKind::WhileLetPattern);
    // `next()` (the condition value) — not covered.
    assert!(!helper.covers(call_pos(&analysis, "next", 0)));
    // inside the loop body — covered.
    assert!(helper.covers(call_pos(&analysis, "helper", 0)));
    // after the loop — not covered.
    assert!(!helper.covers(call_pos(&analysis, "helper", 1)));
}

#[test]
fn a_let_else_binding_is_refutable_and_covers_after_the_statement() {
    let analysis = analyze("fn f() {\nlet Some(v) = o else { return };\nv();\n}\n");
    let v = only_binding(&analysis, "v");
    assert_eq!(v.kind, BindingKind::Let);
    // A `let`...`else` pattern is refutable, so `v` is an ambiguous binding.
    assert!(v.ambiguous);
    assert!(v.covers(call_pos(&analysis, "v", 0)));
}

// ---------------------------------------------------------------------------
// Ambiguity policy
// ---------------------------------------------------------------------------

#[test]
fn refutable_bare_identifiers_are_marked_ambiguous_irrefutable_are_not() {
    let analysis = analyze(
        "fn f() {\n\
         let x = 0;             // irrefutable let\n\
         for y in it {}         // irrefutable for\n\
         match m { z => {}, _ => {} }   // refutable\n\
         if let Ok(w) = r {}    // refutable\n\
         }\n",
    );
    assert!(!only_binding(&analysis, "x").ambiguous);
    assert!(!only_binding(&analysis, "y").ambiguous);
    assert!(only_binding(&analysis, "z").ambiguous);
    assert!(only_binding(&analysis, "w").ambiguous);
}

#[test]
fn variant_and_constant_like_match_arms_are_recorded_as_ambiguous() {
    let analysis = analyze("fn f() { match m { None => {}, CONST => {}, S::V => {} } }\n");
    // `None`/`CONST` bare-identifier arms are recorded as ambiguous bindings —
    // syntax cannot prove they are not unit-variant/const patterns. `S::V` is a
    // path, which is never a binding.
    assert!(only_binding(&analysis, "None").ambiguous);
    assert!(only_binding(&analysis, "CONST").ambiguous);
    assert!(analysis
        .bindings
        .iter()
        .all(|b| b.name != "V" && b.name != "S"));
}

#[test]
fn capture_names_and_struct_shorthand_are_definite_bindings() {
    let analysis =
        analyze("fn f() {\nmatch m {\na @ Some(_) => {},\nS { f } => {},\n_ => {}\n}\n}\n");
    // `a @ ...` always binds `a`; `S { f }` shorthand always binds `f`. Neither
    // is a bare-identifier leaf, so they are not flagged ambiguous even though
    // they sit in a refutable position.
    assert!(!only_binding(&analysis, "a").ambiguous);
    assert!(!only_binding(&analysis, "f").ambiguous);
}

// ---------------------------------------------------------------------------
// Range / source-bounds validation (applies to every emitted binding)
// ---------------------------------------------------------------------------

#[test]
fn every_binding_has_in_bounds_half_open_ranges_and_a_containing_site() {
    let source = "fn f(p: u8) {\nlet (a, b) = t;\nlet g = |c| c;\nfor d in it {}\n\
                  match m { Some(e) if e => {}, _ => {} }\n}\n";
    let analysis = analyze(source);
    assert!(!analysis.bindings.is_empty());
    for binding in &analysis.bindings {
        // name_range is in-bounds, half-open and slices the written name.
        assert!(binding.name_range.byte_start < binding.name_range.byte_end);
        assert!(binding.name_range.byte_end as usize <= source.len());
        assert_eq!(source_of(source, binding.name_range), binding.name);
        // The site contains the bound name.
        assert!(binding.binding_site_range.contains(&binding.name_range));
        // Every visibility range is in-bounds and half-open.
        for range in &binding.visibility_ranges {
            assert!(range.byte_start <= range.byte_end);
            assert!(range.byte_end as usize <= source.len());
        }
    }
}

#[test]
fn utf8_and_crlf_sources_produce_correct_byte_and_column_ranges() {
    // A multi-byte UTF-8 char and CRLF line endings must not skew byte offsets.
    let source = "fn f() {\r\nlet héllo = 1;\r\nhéllo();\r\n}\r\n";
    let analysis = analyze(source);
    let binding = only_binding(&analysis, "héllo");
    assert_eq!(source_of(source, binding.name_range), "héllo");
    assert!(binding.covers(call_pos(&analysis, "héllo", 0)));
    // byte positions are UTF-8 bytes, so `héllo` is 6 bytes, not 5 chars.
    assert_eq!(binding.name_range.byte_len(), 6);
}

// ---------------------------------------------------------------------------
// Incremental equivalence (TASK 1 contract extends to the new facts)
// ---------------------------------------------------------------------------

struct Harness {
    registry: ParserRegistry,
    parser: Parser,
}

impl Harness {
    fn new() -> Self {
        let registry = ParserRegistry::new().expect("registry");
        let mut parser = Parser::new();
        parser
            .set_language(&registry.adapter(LanguageId::Rust).ts_language())
            .expect("grammar loads");
        Self { registry, parser }
    }

    fn run(&mut self, source: &str, edit: &TextEdit) -> repodex::incremental::IncrementalOutcome {
        let adapter = self.registry.adapter(LanguageId::Rust);
        run_case(adapter, &mut self.parser, "probe.rs", source, edit).expect("case runs")
    }

    fn run_sequence(
        &mut self,
        source: &str,
        edits: &[TextEdit],
    ) -> Vec<repodex::incremental::IncrementalOutcome> {
        let adapter = self.registry.adapter(LanguageId::Rust);
        run_sequence(adapter, &mut self.parser, "probe.rs", source, edits).expect("sequence runs")
    }
}

const INC_SRC: &str = "fn helper() {}\nfn run() {\n    let helper = || {};\n    helper();\n}\n";

#[test]
fn incremental_binding_changes_equal_fresh_extraction() {
    let mut harness = Harness::new();
    // Locate `helper` in `let helper = || {};` (the binding name).
    let name_at = INC_SRC.find("let helper").unwrap() + "let ".len();
    // Rename the binding.
    let outcome = harness.run(INC_SRC, &TextEdit::replace(name_at, name_at + 6, "shadow"));
    assert!(
        outcome.equivalent(),
        "binding rename: {:?}",
        outcome.details
    );
    assert_eq!(only_binding(&outcome.full, "shadow").kind, BindingKind::Let);
    // The incremental and fresh analyses carry identical binding facts.
    assert_eq!(outcome.incremental.bindings, outcome.full.bindings);
}

#[test]
fn incremental_binding_insert_and_delete_equal_fresh_extraction() {
    let mut harness = Harness::new();
    let insertion = INC_SRC.find("    helper();").unwrap();
    // Insert a new `let` before the call.
    let outcome = harness.run(
        INC_SRC,
        &TextEdit::insert(insertion, "    let helper = || {};\n"),
    );
    assert!(
        outcome.equivalent(),
        "binding insert: {:?}",
        outcome.details
    );
    // Delete the `let helper` statement entirely.
    let delete_start = INC_SRC.find("    let helper").unwrap();
    let delete_end = INC_SRC.find("    helper();").unwrap();
    let outcome = harness.run(INC_SRC, &TextEdit::delete(delete_start, delete_end));
    assert!(
        outcome.equivalent(),
        "binding delete: {:?}",
        outcome.details
    );
    assert!(outcome.full.bindings_named("helper").next().is_none());
}

#[test]
fn incremental_block_boundary_and_pattern_shape_edits_equal_fresh() {
    let mut harness = Harness::new();
    // Wrap the `let` in an inner block (a block-boundary edit), then change the
    // pattern shape (a pattern-shape edit) — both must equal a fresh parse.
    let body_start = INC_SRC.find("    let helper").unwrap();
    let edits = [
        TextEdit::insert(body_start, "    {\n        "),
        TextEdit::replace(INC_SRC.len() - 3, INC_SRC.len() - 3, "    }\n"),
    ];
    for (index, outcome) in harness.run_sequence(INC_SRC, &edits).iter().enumerate() {
        assert!(outcome.equivalent(), "edit {index}: {:?}", outcome.details);
    }
    // A pattern-shape edit: turn `let helper` into `let (helper, other)`.
    let name_at = INC_SRC.find("let helper").unwrap() + "let ".len();
    let outcome = harness.run(
        INC_SRC,
        &TextEdit::replace(name_at, name_at + 6, "(helper, other)"),
    );
    assert!(outcome.equivalent(), "pattern shape: {:?}", outcome.details);
    assert_eq!(only_binding(&outcome.full, "helper").kind, BindingKind::Let);
    assert_eq!(only_binding(&outcome.full, "other").kind, BindingKind::Let);
}

// ---------------------------------------------------------------------------
// Repository snapshot update-vs-fresh (TASK 3A gate extends to bindings)
// ---------------------------------------------------------------------------

fn write(root: &Path, relative: &str, contents: &[u8]) {
    let target = root.join(relative);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(target, contents).expect("write file");
}

/// A repository whose Rust file carries several binding kinds.
fn binding_scaffold(temp: &TempDir) -> PathBuf {
    let root = temp.path().join("repo");
    write(
        &root,
        "src/lib.rs",
        b"fn helper() {}\nfn run(p: u8) {\n    let helper = || {};\n    helper();\n    for i in it { helper(); }\n}\n",
    );
    write(&root, "src/other.rs", b"fn other() { run(0); }\n");
    root
}

fn store(temp: &TempDir, name: &str) -> PathBuf {
    temp.path().join("store").join(name)
}

fn repo_analyzer() -> Analyzer {
    Analyzer::new(AnalyzerConfig::default()).expect("analyzer")
}

fn build(root: &Path, output: &Path) -> RepositoryManifest {
    build_snapshot(&repo_analyzer(), root, output, BuildOptions::default())
        .expect("build")
        .manifest
}

fn update(root: &Path, previous: &Path, output: &Path) -> RepositoryManifest {
    update_snapshot(
        &repo_analyzer(),
        root,
        previous,
        output,
        BuildOptions::default(),
    )
    .expect("update")
    .manifest
}

fn file_bindings(output: &Path, relative: &str) -> Vec<LocalBindingOccurrence> {
    RepositoryFactIndex::load(output)
        .expect("load index")
        .file(relative)
        .unwrap_or_else(|| panic!("{relative} in index"))
        .bindings
        .clone()
}

#[test]
fn update_vs_fresh_snapshot_equivalence_when_bindings_change() {
    let cases: &[(&str, &[u8])] = &[
        // let added
        (
            "let-added",
            b"fn helper() {}\nfn run(p: u8) {\n    let helper = || {};\n    let extra = 1;\n    helper();\n}\n",
        ),
        // let removed
        (
            "let-removed",
            b"fn helper() {}\nfn run(p: u8) {\n    helper();\n}\n",
        ),
        // parameter renamed
        (
            "param-renamed",
            b"fn helper() {}\nfn run(q: u8) {\n    let helper = || {};\n    helper();\n}\n",
        ),
        // nested block changed (the `let` moves into an inner block)
        (
            "block-changed",
            b"fn helper() {}\nfn run(p: u8) {\n    { let helper = || {}; helper(); }\n    helper();\n}\n",
        ),
    ];
    for (label, mutated) in cases {
        let temp = TempDir::new(label);
        let root = binding_scaffold(&temp);
        let previous = store(&temp, "previous");
        let output = store(&temp, "output");
        build(&root, &previous);

        write(&root, "src/lib.rs", mutated);
        let updated = update(&root, &previous, &output);
        let fresh = build(&root, &store(&temp, "fresh"));

        assert_eq!(
            updated.snapshot_digest, fresh.snapshot_digest,
            "{label}: update must equal fresh snapshot"
        );
        assert_eq!(updated, fresh, "{label}: manifests must be identical");
        // The persisted binding facts themselves are identical, not merely the
        // manifest digest.
        assert_eq!(
            file_bindings(&output, "src/lib.rs"),
            file_bindings(&store(&temp, "fresh"), "src/lib.rs"),
            "{label}: binding facts must round-trip identically"
        );
    }
}

// ---------------------------------------------------------------------------
// Cross-root determinism
// ---------------------------------------------------------------------------

#[test]
fn a_repository_build_is_identical_across_two_absolute_roots() {
    let temp_a = TempDir::new("root-a");
    let temp_b = TempDir::new("root-b");
    let root_a = binding_scaffold(&temp_a);
    let root_b = binding_scaffold(&temp_b);
    let out_a = store(&temp_a, "snap");
    let out_b = store(&temp_b, "snap");

    let manifest_a = build(&root_a, &out_a);
    let manifest_b = build(&root_b, &out_b);
    assert_eq!(manifest_a.snapshot_digest, manifest_b.snapshot_digest);

    // Binding facts and their visibility ranges carry no absolute path.
    assert_eq!(
        file_bindings(&out_a, "src/lib.rs"),
        file_bindings(&out_b, "src/lib.rs")
    );
}

// ---------------------------------------------------------------------------
// Snapshot round-trip: bindings persist exactly through serialize/deserialize
// ---------------------------------------------------------------------------

#[test]
fn bindings_survive_a_snapshot_round_trip() {
    let temp = TempDir::new("rt");
    let root = binding_scaffold(&temp);
    let output = store(&temp, "snap");
    build(&root, &output);
    let bindings = file_bindings(&output, "src/lib.rs");
    assert!(!bindings.is_empty());
    assert!(bindings.iter().any(|b| b.kind == BindingKind::Let));
    assert!(bindings
        .iter()
        .any(|b| b.kind == BindingKind::FunctionParameter));
    assert!(bindings.iter().any(|b| b.kind == BindingKind::ForPattern));
    assert!(bindings.iter().all(|b| !b.name.is_empty()));
    // IDs are dense and deterministic across the round trip.
    for (index, binding) in bindings.iter().enumerate() {
        assert_eq!(binding.binding_id, index as u32);
    }
}
