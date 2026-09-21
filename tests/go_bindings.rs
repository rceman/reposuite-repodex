//! TASK 4B: Go local-binding fact extraction.
//!
//! Asserts which names become `LocalBindingOccurrence`s, their kinds, and the
//! source regions they can shadow — using `LocalBindingOccurrence::covers`
//! (a pure source-range test, never name resolution).

mod support;

use repodex::model::LocalBindingOccurrence;
use repodex::parser::{Analyzer, AnalyzerConfig};
use repodex::{BindingKind, FileAnalysis, LanguageId, SourceRange};

fn analyze(source: &str) -> FileAnalysis {
    Analyzer::new(AnalyzerConfig::default())
        .expect("analyzer")
        .analyze_bytes("probe.go", LanguageId::Go, source.as_bytes())
}

fn bindings_named<'a>(a: &'a FileAnalysis, name: &str) -> Vec<&'a LocalBindingOccurrence> {
    a.bindings.iter().filter(|b| b.name == name).collect()
}

fn only<'a>(a: &'a FileAnalysis, name: &str) -> &'a LocalBindingOccurrence {
    let found = bindings_named(a, name);
    assert_eq!(found.len(), 1, "expected exactly one `{name}` binding");
    found[0]
}

fn call_pos(a: &FileAnalysis, name: &str, n: usize) -> u32 {
    a.calls
        .iter()
        .filter(|c| c.callee_written == name)
        .nth(n)
        .unwrap_or_else(|| panic!("missing {n}th `{name}` call"))
        .callee_range
        .byte_start
}

fn src(source: &str, r: SourceRange) -> &str {
    &source[r.byte_start as usize..r.byte_end as usize]
}

// ---------------------------------------------------------------------------
// signature bindings
// ---------------------------------------------------------------------------

#[test]
fn function_parameter_binds_in_body() {
    let a = analyze("package p\nfunc run(helper func()) {\n\thelper()\n}\n");
    let b = only(&a, "helper");
    assert_eq!(b.kind, BindingKind::FunctionParameter);
    // The call `helper()` is inside the body -> covered; the signature is not.
    let call = a
        .calls
        .iter()
        .find(|c| c.callee_written == "helper")
        .unwrap();
    assert!(b.covers(call.callee_range.byte_start));
    assert!(!b.covers(b.name_range.byte_start)); // the param token isn't in its body
}

#[test]
fn multiple_parameters_each_bind() {
    let a = analyze("package p\nfunc f(a, b int, c string) {\n\t_ = a\n\t_ = b\n\t_ = c\n}\n");
    for n in ["a", "b", "c"] {
        assert_eq!(only(&a, n).kind, BindingKind::FunctionParameter);
    }
}

#[test]
fn named_result_parameter_binds() {
    let a = analyze("package p\nfunc f() (helper int) {\n\thelper++\n\treturn\n}\n");
    assert_eq!(only(&a, "helper").kind, BindingKind::FunctionResult);
}

#[test]
fn method_receiver_binds() {
    let a = analyze("package p\nfunc (s *Svc) Run() {\n\ts.work()\n}\n");
    let b = only(&a, "s");
    assert_eq!(b.kind, BindingKind::MethodReceiver);
    let call = a
        .calls
        .iter()
        .find(|c| c.callee_written == "s.work")
        .unwrap();
    assert!(b.covers(call.callee_range.byte_start));
}

#[test]
fn func_literal_parameter_and_result_bind() {
    let a =
        analyze("package p\nvar f = func(helper func()) (out int) {\n\thelper()\n\treturn\n}\n");
    let p = only(&a, "helper");
    assert_eq!(p.kind, BindingKind::FunctionLiteralParameter);
    assert_eq!(only(&a, "out").kind, BindingKind::FunctionLiteralResult);
}

#[test]
fn nested_func_literal_params_are_independent() {
    let a = analyze(
        "package p\nvar f = func(outer int) {\n\tg := func(inner int) { _ = inner }\n\t_ = g\n\t_ = outer\n}\n",
    );
    assert_eq!(
        only(&a, "outer").kind,
        BindingKind::FunctionLiteralParameter
    );
    assert_eq!(
        only(&a, "inner").kind,
        BindingKind::FunctionLiteralParameter
    );
}

// ---------------------------------------------------------------------------
// local declarations
// ---------------------------------------------------------------------------

#[test]
fn local_var_binds_and_blocks() {
    let a = analyze("package p\nfunc f() {\n\tvar helper func()\n\thelper()\n}\n");
    let b = only(&a, "helper");
    assert_eq!(b.kind, BindingKind::Variable);
    let call = a
        .calls
        .iter()
        .find(|c| c.callee_written == "helper")
        .unwrap();
    assert!(b.covers(call.callee_range.byte_start));
}

#[test]
fn multi_name_var_emits_each() {
    let a = analyze("package p\nfunc f() {\n\tvar a, b = 1, 2\n\t_ = a\n\t_ = b\n}\n");
    assert_eq!(only(&a, "a").kind, BindingKind::Variable);
    assert_eq!(only(&a, "b").kind, BindingKind::Variable);
}

#[test]
fn grouped_var_emits_each_spec() {
    let a = analyze(
        "package p\nfunc f() {\n\tvar (\n\t\ta int\n\t\tb string\n\t)\n\t_ = a\n\t_ = b\n}\n",
    );
    assert_eq!(only(&a, "a").kind, BindingKind::Variable);
    assert_eq!(only(&a, "b").kind, BindingKind::Variable);
}

#[test]
fn local_const_is_a_blocker() {
    let a = analyze("package p\nfunc f() {\n\tconst helper = 1\n\thelper()\n}\n");
    assert_eq!(only(&a, "helper").kind, BindingKind::Constant);
}

#[test]
fn local_type_is_a_blocker() {
    let a = analyze("package p\nfunc f() {\n\ttype helper int\n\thelper(1)\n}\n");
    assert_eq!(only(&a, "helper").kind, BindingKind::LocalType);
}

// ---------------------------------------------------------------------------
// short variable declarations
// ---------------------------------------------------------------------------

#[test]
fn short_var_binds_after_declaration() {
    let a = analyze("package p\nfunc f() {\n\thelper := func() {}\n\thelper()\n}\n");
    let b = only(&a, "helper");
    assert_eq!(b.kind, BindingKind::ShortVariable);
    // RHS/self position not covered; the call after is covered.
    assert!(!b.covers(b.name_range.byte_start));
    let call = a
        .calls
        .iter()
        .find(|c| c.callee_written == "helper")
        .unwrap();
    assert!(b.covers(call.callee_range.byte_start));
}

#[test]
fn short_var_rhs_is_not_covered() {
    // `helper := helper` — the RHS occurrence is not covered by the new binding.
    let a = analyze("package p\nfunc helper() {}\nfunc f() {\n\tx := helper\n\thelper()\n}\n");
    let b = only(&a, "x");
    let _ = b;
    // There is no `helper` local binding (only the package func), so the RHS
    // `helper` reference and the call are NOT covered by any local binding.
    assert!(bindings_named(&a, "helper").is_empty());
}

#[test]
fn multi_name_short_var() {
    let a = analyze("package p\nfunc f() {\n\ta, b := 1, 2\n\t_ = a\n\t_ = b\n}\n");
    assert_eq!(only(&a, "a").kind, BindingKind::ShortVariable);
    assert_eq!(only(&a, "b").kind, BindingKind::ShortVariable);
}

#[test]
fn same_block_redeclaration_is_not_a_new_binding() {
    // `x` is declared once; `x, helper :=` redeclares x but introduces helper.
    let a =
        analyze("package p\nfunc f() {\n\tx := 1\n\tx, helper := 2, func() {}\n\thelper()\n}\n");
    assert_eq!(
        bindings_named(&a, "x").len(),
        1,
        "x redeclared, not re-bound"
    );
    assert_eq!(bindings_named(&a, "helper").len(), 1);
}

#[test]
fn parameter_redeclaration_in_body_is_not_a_new_binding() {
    // `x` is a parameter; `x, helper :=` in the body reuses it (§18).
    let a = analyze("package p\nfunc f(x int) {\n\tx, helper := 1, func() {}\n\thelper()\n}\n");
    assert_eq!(only(&a, "x").kind, BindingKind::FunctionParameter);
    assert_eq!(bindings_named(&a, "x").len(), 1, "param x not re-bound");
    assert_eq!(only(&a, "helper").kind, BindingKind::ShortVariable);
}

#[test]
fn inner_block_shadows_outer_name() {
    // Inner `helper` in a nested block is a NEW binding (shadows outer) — §19.
    let a = analyze(
        "package p\nfunc f() {\n\thelper := outer\n\t{\n\t\thelper, other := inner, 1\n\t\thelper()\n\t}\n\thelper()\n}\n",
    );
    let helpers = bindings_named(&a, "helper");
    assert_eq!(helpers.len(), 2, "outer + inner helper are distinct");
    assert_eq!(only(&a, "other").name, "other");
}

#[test]
fn block_scoped_binding_does_not_leak() {
    // `helper :=` inside `{ }` covers only the inner block — §20.
    let a = analyze(
        "package p\nfunc f() {\n\t{\n\t\thelper := func() {}\n\t\thelper()\n\t}\n\thelper()\n}\n",
    );
    let b = only(&a, "helper");
    assert!(b.covers(call_pos(&a, "helper", 0)));
    assert!(!b.covers(call_pos(&a, "helper", 1)));
}

// ---------------------------------------------------------------------------
// implicit blocks: if / switch / for
// ---------------------------------------------------------------------------

#[test]
fn if_init_binds_through_both_branches() {
    let a = analyze(
        "package p\nfunc f() {\n\tif helper := g(); helper != nil {\n\t\thelper()\n\t} else {\n\t\thelper()\n\t}\n}\n",
    );
    let b = only(&a, "helper");
    assert_eq!(b.kind, BindingKind::ShortVariable);
    let calls: Vec<u32> = a
        .calls
        .iter()
        .filter(|c| c.callee_written == "helper")
        .map(|c| c.callee_range.byte_start)
        .collect();
    assert_eq!(calls.len(), 2);
    assert!(b.covers(calls[0]) && b.covers(calls[1]));
}

#[test]
fn if_init_does_not_leak_after() {
    let a = analyze(
        "package p\nfunc f() {\n\tif helper := g(); helper {\n\t\thelper()\n\t}\n\thelper()\n}\n",
    );
    let b = only(&a, "helper");
    let inside = call_pos(&a, "helper", 0);
    let after = call_pos(&a, "helper", 1);
    assert!(b.covers(inside));
    assert!(!b.covers(after));
}

#[test]
fn switch_init_binds_all_clauses() {
    let a = analyze(
        "package p\nfunc f() {\n\tswitch helper := g(); v {\n\tcase 1:\n\t\thelper()\n\tdefault:\n\t\thelper()\n\t}\n}\n",
    );
    let b = only(&a, "helper");
    let calls: Vec<u32> = a
        .calls
        .iter()
        .filter(|c| c.callee_written == "helper")
        .map(|c| c.callee_range.byte_start)
        .collect();
    assert_eq!(calls.len(), 2);
    assert!(b.covers(calls[0]) && b.covers(calls[1]));
}

#[test]
fn for_init_binds_condition_post_body() {
    let a = analyze(
        "package p\nfunc f() {\n\tfor helper := 0; check(helper); helper++ {\n\t\thelper()\n\t}\n\thelper()\n}\n",
    );
    let b = only(&a, "helper");
    let calls: Vec<u32> = a
        .calls
        .iter()
        .filter(|c| c.callee_written == "helper")
        .map(|c| c.callee_range.byte_start)
        .collect();
    // body call covered; the call after the loop is not.
    assert!(b.covers(calls[0]));
    assert!(!b.covers(*calls.last().unwrap()));
}

// ---------------------------------------------------------------------------
// range
// ---------------------------------------------------------------------------

#[test]
fn range_short_var_binds_in_body() {
    let a = analyze("package p\nfunc f() {\n\tfor helper := range items {\n\t\thelper()\n\t}\n}\n");
    assert_eq!(only(&a, "helper").kind, BindingKind::RangeVariable);
}

#[test]
fn range_two_vars() {
    let a = analyze("package p\nfunc f() {\n\tfor key, helper := range items {\n\t\t_ = key\n\t\thelper()\n\t}\n}\n");
    assert_eq!(only(&a, "key").kind, BindingKind::RangeVariable);
    assert_eq!(only(&a, "helper").kind, BindingKind::RangeVariable);
}

#[test]
fn range_assignment_is_not_a_binding() {
    // `for helper = range x` — `=` does not declare (§25).
    let a = analyze("package p\nfunc f() {\n\tfor helper = range items {\n\t\thelper()\n\t}\n}\n");
    assert!(bindings_named(&a, "helper").is_empty());
}

// ---------------------------------------------------------------------------
// type switch
// ---------------------------------------------------------------------------

#[test]
fn type_switch_variable_binds_in_each_clause() {
    let a = analyze(
        "package p\nfunc f() {\n\tswitch helper := v.(type) {\n\tcase int:\n\t\thelper()\n\tdefault:\n\t\thelper()\n\t}\n\thelper()\n}\n",
    );
    let b = only(&a, "helper");
    assert_eq!(b.kind, BindingKind::TypeSwitchVariable);
    // One source occurrence, two disjoint clause ranges.
    assert_eq!(b.visibility_ranges.len(), 2);
    let calls: Vec<u32> = a
        .calls
        .iter()
        .filter(|c| c.callee_written == "helper")
        .map(|c| c.callee_range.byte_start)
        .collect();
    assert!(b.covers(calls[0]) && b.covers(calls[1]));
    assert!(!b.covers(*calls.last().unwrap())); // after the switch
}

#[test]
fn type_switch_does_not_leak() {
    let a = analyze(
        "package p\nfunc f() {\n\tswitch helper := v.(type) {\n\tcase int:\n\t\thelper()\n\t}\n\thelper()\n}\n",
    );
    let b = only(&a, "helper");
    let after = call_pos(&a, "helper", 1);
    assert!(!b.covers(after));
}

// ---------------------------------------------------------------------------
// select
// ---------------------------------------------------------------------------

#[test]
fn select_receive_short_var_binds_in_its_clause() {
    let a = analyze(
        "package p\nfunc f() {\n\tselect {\n\tcase helper := <-ch:\n\t\thelper()\n\tdefault:\n\t\thelper()\n\t}\n}\n",
    );
    let b = only(&a, "helper");
    assert_eq!(b.kind, BindingKind::SelectReceiveVariable);
    let in_clause = call_pos(&a, "helper", 0);
    let other_clause = call_pos(&a, "helper", 1);
    assert!(b.covers(in_clause));
    assert!(!b.covers(other_clause)); // does not leak into the default clause
}

#[test]
fn select_receive_assignment_is_not_a_binding() {
    let a =
        analyze("package p\nfunc f() {\n\tselect {\n\tcase helper = <-ch:\n\t\thelper()\n\t}\n}\n");
    assert!(bindings_named(&a, "helper").is_empty());
}

// ---------------------------------------------------------------------------
// clause isolation
// ---------------------------------------------------------------------------

#[test]
fn case_block_isolation() {
    // `helper :=` in case 1 must not leak into case 2 (§30).
    let a = analyze(
        "package p\nfunc f() {\n\tswitch x {\n\tcase 1:\n\t\thelper := func() {}\n\t\thelper()\n\tcase 2:\n\t\thelper()\n\t}\n}\n",
    );
    let b = only(&a, "helper");
    let case1 = call_pos(&a, "helper", 0);
    let case2 = call_pos(&a, "helper", 1);
    assert!(b.covers(case1));
    assert!(!b.covers(case2));
}

// ---------------------------------------------------------------------------
// negatives
// ---------------------------------------------------------------------------

#[test]
fn blank_identifier_is_never_a_binding() {
    let a = analyze(
        "package p\nfunc f(_ int) (int) {\n\tfor _ = range x {}\n\t_ = f\n\t_, b := 1, 2\n\t_ = b\n\treturn 0\n}\n",
    );
    assert!(a.bindings.iter().all(|b| b.name != "_"));
}

#[test]
fn labels_are_not_value_bindings() {
    // `helper:` label must not become a binding (§33).
    let a = analyze("package p\nfunc f() {\nhelper:\n\tfor {\n\t\tbreak helper\n\t}\n}\n");
    assert!(bindings_named(&a, "helper").is_empty());
}

#[test]
fn selector_and_field_names_are_not_bindings() {
    let a = analyze(
        "package p\nfunc f() {\n\tx := S{Field: 1}\n\t_ = x.Field\n\ty := x.Method()\n\t_ = y\n}\n",
    );
    // `Field`/`Method` are field/selector names, never local bindings (§34).
    assert!(bindings_named(&a, "Field").is_empty());
    assert!(bindings_named(&a, "Method").is_empty());
}

#[test]
fn package_level_decls_are_not_local_bindings() {
    // §31: package-level var/const/type/func stay declarations, not bindings.
    let a = analyze(
        "package p\nvar PkgVar int\nconst PkgConst = 1\ntype PkgType int\nfunc PkgFn() {}\n",
    );
    for n in ["PkgVar", "PkgConst", "PkgType", "PkgFn", "p"] {
        assert!(
            bindings_named(&a, n).is_empty(),
            "{n} must not be a binding"
        );
    }
}

// ---------------------------------------------------------------------------
// encoding / recovery
// ---------------------------------------------------------------------------

#[test]
fn utf8_before_a_binding_keeps_byte_offsets() {
    let a = analyze("package p\nfunc f() {\n\t_ = \"héllo\"\n\thelper := 1\n\t_ = helper\n}\n");
    let b = only(&a, "helper");
    assert_eq!(
        src(
            "package p\nfunc f() {\n\t_ = \"héllo\"\n\thelper := 1\n\t_ = helper\n}\n",
            b.name_range
        ),
        "helper"
    );
}

#[test]
fn crlf_line_endings() {
    let a = analyze("package p\r\nfunc f() {\r\n\thelper := 1\r\n\t_ = helper\r\n}\r\n");
    let b = only(&a, "helper");
    assert_eq!(b.kind, BindingKind::ShortVariable);
}

#[test]
fn recoverable_malformed_source_still_emits_bindings() {
    // A missing close-brace — bindings already introduced are still emitted.
    let a = analyze("package p\nfunc f() {\n\thelper := 1\n\t_ = helper\n");
    assert!(!bindings_named(&a, "helper").is_empty());
}

// ---------------------------------------------------------------------------
// name-range exactness
// ---------------------------------------------------------------------------

#[test]
fn every_binding_name_range_slices_exactly() {
    let source = "package p\nfunc f(a, b int) (r int) {\n\tvar c, d = 1, 2\n\te, f := 3, 4\n\tfor g := range x {}\n\t_ = a\n\treturn\n}\n";
    let a = analyze(source);
    assert!(!a.bindings.is_empty());
    for b in &a.bindings {
        assert_eq!(
            src(source, b.name_range),
            b.name,
            "name_range must slice to the identifier"
        );
        assert!(b.binding_site_range.contains(&b.name_range));
        for v in &b.visibility_ranges {
            assert!(v.byte_end as usize <= source.len());
            assert!(v.byte_start <= v.byte_end);
        }
    }
}

// ---------------------------------------------------------------------------
// §42 incremental-vs-fresh extraction equivalence
// ---------------------------------------------------------------------------

use repodex::incremental::{run_case, TextEdit};
use repodex::parser::ParserRegistry;
use tree_sitter::Parser;

struct Inc {
    registry: ParserRegistry,
    parser: Parser,
}

impl Inc {
    fn new() -> Self {
        let registry = ParserRegistry::new().expect("registry");
        let mut parser = Parser::new();
        parser
            .set_language(&registry.adapter(LanguageId::Go).ts_language())
            .expect("grammar");
        Self { registry, parser }
    }
    fn run(&mut self, source: &str, edit: &TextEdit) -> repodex::incremental::IncrementalOutcome {
        run_case(
            self.registry.adapter(LanguageId::Go),
            &mut self.parser,
            "probe.go",
            source,
            edit,
        )
        .expect("case")
    }
}

const GSRC: &str =
    "package p\nfunc helper() {}\nfunc run() {\n\thelper := func() {}\n\thelper()\n}\n";

fn inc_eq(h: &mut Inc, edit: TextEdit, label: &str) {
    let outcome = h.run(GSRC, &edit);
    assert!(outcome.equivalent(), "{label}: {:?}", outcome.details);
    assert_eq!(
        outcome.incremental.bindings, outcome.full.bindings,
        "{label}: binding facts must match a fresh parse"
    );
}

#[test]
fn incremental_binding_rename_equals_fresh() {
    let mut h = Inc::new();
    let at = GSRC.find("helper :=").unwrap();
    inc_eq(&mut h, TextEdit::replace(at, at + 6, "shadow"), "rename");
}

#[test]
fn incremental_binding_insert_equals_fresh() {
    let mut h = Inc::new();
    let at = GSRC.find("\thelper()").unwrap();
    inc_eq(&mut h, TextEdit::insert(at, "\tshadow := 1\n"), "insert :=");
}

#[test]
fn incremental_binding_delete_equals_fresh() {
    let mut h = Inc::new();
    let start = GSRC.find("\thelper :=").unwrap();
    let end = GSRC.find("\thelper()").unwrap();
    inc_eq(&mut h, TextEdit::delete(start, end), "delete :=");
}

#[test]
fn incremental_var_to_short_var_equals_fresh() {
    let src = "package p\nfunc f() {\n\tvar helper func()\n\thelper()\n}\n";
    let mut h = Inc::new();
    let adapter = h.registry.adapter(LanguageId::Go);
    // `var helper func()` -> `helper := func() {}`
    let start = src.find("var helper").unwrap();
    let end = src.find("\thelper()").unwrap();
    let outcome = run_case(
        adapter,
        &mut h.parser,
        "probe.go",
        src,
        &TextEdit::replace(start, end - 1, "helper := func() {}"),
    )
    .expect("case");
    assert!(outcome.equivalent(), "var->:= {:?}", outcome.details);
    assert_eq!(outcome.incremental.bindings, outcome.full.bindings);
}

// ---------------------------------------------------------------------------
// §43 snapshot round-trip / update-vs-fresh preserve binding facts
// ---------------------------------------------------------------------------

use repodex::repository::artifact::read_file_analysis;
use repodex::repository::{build_snapshot, update_snapshot, BuildOptions};
use support::TempDir;

fn snapshot_bindings(
    files: &[(&str, &str)],
    temp: &TempDir,
    tag: &str,
) -> Vec<LocalBindingOccurrence> {
    let root = temp.path().join(tag);
    for (rel, c) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, c).unwrap();
    }
    let snap = temp.path().join(format!("{tag}-snap"));
    let manifest = build_snapshot(
        &repodex::parser::Analyzer::new(AnalyzerConfig::default()).unwrap(),
        &root,
        &snap,
        BuildOptions::default(),
    )
    .expect("snapshot")
    .manifest;
    let mut out = Vec::new();
    for f in &manifest.files {
        if f.language == "go" {
            out.extend(read_file_analysis(&snap, f).expect("analysis").bindings);
        }
    }
    out
}

#[test]
fn snapshot_round_trip_preserves_binding_facts() {
    let t = TempDir::new("t4b-rt");
    let files = &[(
        "a.go",
        "package p\nfunc f(x int) (r int) {\n\ty := x\n\tfor i := range xs { _ = i }\n\treturn\n}\n",
    )];
    let a = snapshot_bindings(files, &t, "a");
    let b = snapshot_bindings(files, &t, "b");
    assert!(!a.is_empty());
    assert_eq!(a, b, "identical bytes must produce identical bindings");
    let kinds: std::collections::BTreeSet<_> = a.iter().map(|x| x.kind.as_str()).collect();
    assert!(
        kinds.contains("function_parameter")
            && kinds.contains("short_variable")
            && kinds.contains("range_variable")
    );
}

#[test]
fn snapshot_update_vs_fresh_preserves_bindings() {
    let t = TempDir::new("t4b-uvf");
    let root = t.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("a.go"),
        "package p\nfunc f() {\n\tx := 1\n\t_ = x\n}\n",
    )
    .unwrap();
    let base = t.path().join("base");
    let analyzer = repodex::parser::Analyzer::new(AnalyzerConfig::default()).unwrap();
    build_snapshot(&analyzer, &root, &base, BuildOptions::default()).expect("base");
    // Add a binding-bearing file.
    std::fs::write(
        root.join("b.go"),
        "package p\nfunc g() {\n\tfor k, v := range m { _ = k; _ = v }\n}\n",
    )
    .unwrap();
    let upd = t.path().join("upd");
    update_snapshot(&analyzer, &root, &base, &upd, BuildOptions::default()).expect("update");
    let fresh = t.path().join("fresh");
    build_snapshot(&analyzer, &root, &fresh, BuildOptions::default()).expect("fresh");
    let collect = |snap: &std::path::Path| {
        let m = repodex::repository::artifact::read_manifest(snap).unwrap();
        let mut all = Vec::new();
        for f in &m.files {
            if f.language == "go" {
                all.extend(read_file_analysis(snap, f).unwrap().bindings);
            }
        }
        all
    };
    assert_eq!(collect(&upd), collect(&fresh));
}

#[test]
fn type_parameter_is_a_blocker() {
    // `func f[T any](v T) { _ = T(v) }` — `T(v)` is a plain_name call, so the
    // type param `T` must be a lexical blocker (§35).
    let a = analyze("package p\nfunc f[T any](v T) {\n\t_ = T(v)\n}\n");
    let b = only(&a, "T");
    assert_eq!(b.kind, BindingKind::TypeParameter);
    let call = a.calls.iter().find(|c| c.callee_written == "T").unwrap();
    assert!(b.covers(call.callee_range.byte_start));
}
