//! Python adapter correctness tests.

mod support;

use repodex::{
    CallLikeForm, DeclarationFlag, DeclarationKind, ImportForm, ReferenceKind, ScopeKind,
};
use support::{
    analyze_fixture, call_summary, declaration_summary, reference_summary, slice, test_evidence,
};

#[test]
fn declarations_cover_required_constructs() {
    let analysis = analyze_fixture("python/declarations.py");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Clean);
    assert_eq!(
        declaration_summary(&analysis),
        vec![
            "variable MODULE_X (file)",
            "variable MODULE_Y (file)",
            "constant MODULE_Z (file)",
            "variable MODULE_A (file)",
            "variable MODULE_B (file)",
            "class Base (file)",
            "field class_attr (file)::Base",
            "method __init__ (file)::Base",
            "method fetch (file)::Base async",
            "method helper (file)::Base",
            "class Child (file)",
            "method method (file)::Child",
            "function top_async (file) async",
            "function top (file)",
            "function nested (file)::top",
            "class Nested (file)::top",
        ]
    );
}

#[test]
fn uppercase_names_are_not_assumed_to_be_constants() {
    let analysis = analyze_fixture("python/declarations.py");
    let plain = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "MODULE_X")
        .expect("MODULE_X");
    assert_eq!(plain.kind, DeclarationKind::Variable);
    // Only an explicit `Final` annotation produces a constant.
    let annotated = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "MODULE_Z")
        .expect("MODULE_Z");
    assert_eq!(annotated.kind, DeclarationKind::Constant);
    let plain_annotated = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "MODULE_Y")
        .expect("MODULE_Y");
    assert_eq!(plain_annotated.kind, DeclarationKind::Variable);
    // The fixture's own `UPPERCASE_NAME` in boundaries.py follows the same rule.
    let boundaries = analyze_fixture("python/boundaries.py");
    let uppercase = boundaries
        .declarations
        .iter()
        .find(|declaration| declaration.name == "UPPERCASE_NAME")
        .expect("UPPERCASE_NAME");
    assert_eq!(uppercase.kind, DeclarationKind::Variable);
}

#[test]
fn nested_definitions_keep_containment() {
    let analysis = analyze_fixture("python/declarations.py");
    let nested_function = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "nested")
        .expect("nested");
    assert_eq!(analysis.scope_path(nested_function.scope_id), "(file)::top");
    let nested_class = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Nested")
        .expect("Nested");
    assert_eq!(analysis.scope_path(nested_class.scope_id), "(file)::top");
    assert!(nested_class.kind == DeclarationKind::Class);
}

#[test]
fn async_flag_and_method_detection() {
    let analysis = analyze_fixture("python/declarations.py");
    let fetch = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "fetch")
        .expect("fetch");
    assert!(fetch.flags.contains(&DeclarationFlag::Async));
    assert_eq!(fetch.kind, DeclarationKind::Method);
    let top_async = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "top_async")
        .expect("top_async");
    assert!(top_async.flags.contains(&DeclarationFlag::Async));
    assert_eq!(top_async.kind, DeclarationKind::Function);
}

#[test]
fn imports_cover_required_forms() {
    let analysis = analyze_fixture("python/declarations.py");
    let rendered = analysis
        .imports
        .iter()
        .map(|import| {
            let items = import
                .items
                .iter()
                .map(|item| match &item.alias {
                    Some(alias) => format!("{} as {alias}", item.target),
                    None => item.target.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{} module={} relative={} items=[{items}]",
                import.form.as_str(),
                import.module.as_deref().unwrap_or("-"),
                import
                    .relative_levels
                    .map(|levels| levels.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![
            "single module=- relative=- items=[os]",
            "grouped module=- relative=- items=[os, sys]",
            "single module=- relative=- items=[numpy as np]",
            "single module=- relative=1 items=[sibling]",
            "single module=pkg relative=2 items=[thing]",
            "grouped module=pkg.sub relative=- items=[first, second as renamed]",
            "wildcard module=pkg relative=- items=[*]",
            "single module=typing relative=- items=[Final]",
        ]
    );
    let wildcard = analysis
        .imports
        .iter()
        .find(|import| import.form == ImportForm::Wildcard)
        .expect("wildcard import");
    assert!(wildcard.items[0].wildcard);
    assert_eq!(wildcard.items[0].target, "*");
}

#[test]
fn decorators_and_bases_are_unresolved_references() {
    let analysis = analyze_fixture("python/declarations.py");
    assert_eq!(
        reference_summary(&analysis),
        vec![
            "decorator decorator",
            "decorator decorator_with_args",
            "base_class Base",
            "base_class mixins.Other",
        ]
    );
    // A base class expression is never resolved to a class declaration.
    let base = analysis
        .references
        .iter()
        .find(|reference| reference.written == "mixin.Other" || reference.written == "mixins.Other")
        .expect("base class");
    assert_eq!(base.kind, ReferenceKind::BaseClass);
}

#[test]
fn definition_time_expressions_are_attributed_to_the_enclosing_scope() {
    let analysis = analyze_fixture("python/declarations.py");
    // `default_value()` is written inside `__init__`'s parameter list but runs
    // when the class body executes, so it belongs to the class scope.
    let default_value = analysis
        .calls
        .iter()
        .find(|call| call.callee_written == "default_value")
        .expect("default_value");
    assert_eq!(analysis.scope_path(default_value.scope_id), "(file)::Base");
    let default_for_top = analysis
        .calls
        .iter()
        .find(|call| call.callee_written == "default_for_top")
        .expect("default_for_top");
    assert_eq!(analysis.scope_path(default_for_top.scope_id), "(file)");
}

#[test]
fn call_forms_cover_plain_attribute_chained_and_indirect() {
    let analysis = analyze_fixture("python/calls.py");
    assert_eq!(
        call_summary(&analysis),
        vec![
            "plain_name plain dyn=false nullsafe=false",
            "member_selector obj.attribute_call dyn=false nullsafe=false",
            "member_selector obj.chained dyn=false nullsafe=false",
            "member_selector obj.chained().call dyn=false nullsafe=false",
            "plain_name get_callable dyn=false nullsafe=false",
            "plain_name indirect dyn=false nullsafe=false",
            "plain_name getattr dyn=false nullsafe=false",
            "indirect getattr(obj, \"method\") dyn=false nullsafe=false",
            "plain_name render dyn=false nullsafe=false",
            "plain_name transform dyn=false nullsafe=false",
            "plain_name len dyn=false nullsafe=false",
        ]
    );
    // `indirect(5)` is call-shaped; nothing claims that `indirect` is a function.
    assert!(analysis
        .calls
        .iter()
        .any(|call| call.callee_written == "indirect" && call.form == CallLikeForm::PlainName));
    // A call inside an f-string expression and inside a comprehension is found.
    assert!(analysis
        .calls
        .iter()
        .any(|call| call.callee_written == "render"));
    assert!(analysis
        .calls
        .iter()
        .any(|call| call.callee_written == "transform"));
}

#[test]
fn lambda_and_boundary_constructs() {
    let analysis = analyze_fixture("python/boundaries.py");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Clean);
    // `MonkeyTarget.method = lambda self: None` declares nothing but still
    // produces an anonymous lambda scope.
    let lambdas: Vec<_> = analysis
        .scopes
        .iter()
        .filter(|scope| scope.kind == ScopeKind::Lambda)
        .collect();
    assert_eq!(lambdas.len(), 1);
    assert!(lambdas[0].name.is_none());
    assert!(!analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "method"
            && declaration.kind == DeclarationKind::Field));
    // `global`/`nonlocal` statements and `match` bindings are not declarations.
    for name in ["value", "module"] {
        assert!(
            !analysis.declarations.iter().any(|declaration| {
                declaration.name == name && declaration.kind != DeclarationKind::Function
            }),
            "`{name}` must not become a declaration"
        );
    }
    assert!(analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "UPPERCASE_NAME"
            && declaration.kind == DeclarationKind::Variable));
    // Dynamic import and `getattr(...)()` are recorded as calls only.
    assert!(analysis
        .calls
        .iter()
        .any(|call| call.callee_written == "getattr"));
    assert!(analysis
        .calls
        .iter()
        .any(|call| call.form == CallLikeForm::Indirect));
}

#[test]
fn aliased_test_case_base_is_not_detected_as_test_evidence() {
    let analysis = analyze_fixture("python/boundaries.py");
    let aliased = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "AliasedCase")
        .expect("AliasedCase");
    // `TestCase as AliasedTestCase` is recorded as an import alias, and the base
    // clause is recorded as written. No resolution happens, so no test evidence
    // is claimed.
    assert!(aliased.test_evidence.is_empty());
    // The method name convention still applies.
    assert_eq!(
        test_evidence(&analysis, "test_aliased"),
        vec!["function_name_convention:test_ prefix"]
    );
}

#[test]
fn test_evidence_covers_pytest_and_unittest_candidates() {
    let analysis = analyze_fixture("python/test_sample.py");
    assert_eq!(
        test_evidence(&analysis, "test_plain"),
        vec!["function_name_convention:test_ prefix"]
    );
    assert_eq!(
        test_evidence(&analysis, "test_parametrized"),
        vec![
            "function_name_convention:test_ prefix",
            "decorator:@pytest.mark.parametrize(\"value\", [1, 2])"
        ]
    );
    assert!(test_evidence(&analysis, "helper_not_a_test").is_empty());
    assert!(test_evidence(&analysis, "helper_method").is_empty());
    // unittest-style: the base clause is evidence on the class.
    assert_eq!(
        test_evidence(&analysis, "LegacyCase"),
        vec!["class_base_syntax:TestCase"]
    );
    assert!(test_evidence(&analysis, "TestGroup").is_empty());
    // A `test_` method in a class that is not a TestCase is still a candidate by
    // name convention only; RepoDex never claims pytest would collect it.
    assert_eq!(
        test_evidence(&analysis, "test_looking"),
        vec!["function_name_convention:test_ prefix"]
    );
    assert_eq!(analysis.test_candidate_count(), 6);
    assert_eq!(analysis.file_test_evidence.len(), 1);
    assert_eq!(analysis.file_test_evidence[0].detail, "test_*.py prefix");
}

#[test]
fn ranges_and_containment_are_source_exact() {
    let analysis = analyze_fixture("python/declarations.py");
    for declaration in &analysis.declarations {
        let name = slice(
            "python/declarations.py",
            declaration.name_range.byte_start,
            declaration.name_range.byte_end,
        );
        assert_eq!(name, declaration.name);
    }
    // A decorated class range includes its decorators.
    let base = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Base")
        .expect("Base");
    let rendered = slice(
        "python/declarations.py",
        base.range.byte_start,
        base.range.byte_end,
    );
    assert!(rendered.starts_with("@decorator\n@decorator_with_args(1)\nclass Base:"));
}

#[test]
fn malformed_source_reports_recovery_and_keeps_neighbours() {
    let analysis = analyze_fixture("python/malformed.py");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Recovered);
    let names: Vec<&str> = analysis
        .declarations
        .iter()
        .map(|declaration| declaration.name.as_str())
        .collect();
    assert!(names.contains(&"valid_before"));
    assert!(names.contains(&"valid_after"));
    // The unterminated `class Incomplete(` declares nothing.
    assert!(!names.contains(&"Incomplete"));
    assert!(analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.kind == repodex::DiagnosticKind::SyntaxError));
}
