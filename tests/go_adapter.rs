//! Go adapter correctness tests.

mod support;

use repodex::{
    CallLikeForm, DeclarationFlag, DeclarationKind, ImportCategory, ReferenceKind, ScopeKind,
};
use support::{
    analyze_fixture, call_summary, declaration_summary, fixture_source, reference_summary, slice,
    test_evidence,
};

#[test]
fn declarations_cover_required_constructs() {
    let analysis = analyze_fixture("go/declarations.go");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Clean);
    assert_eq!(
        declaration_summary(&analysis),
        vec![
            "package fixture (file)",
            "named_type UserID (file)",
            "type_alias Alias (file)",
            "struct Box (file)",
            "field Value (file)::Box",
            "struct Pair (file)",
            "field Key (file)::Pair",
            "field Value (file)::Pair",
            "interface Reader (file)",
            "method Read (file)::Reader",
            "constant StatusA (file)",
            "constant StatusB (file)",
            "constant StatusC (file)",
            "constant StatusD (file)",
            "constant Single (file)",
            "variable Global (file)",
            "variable X (file)",
            "variable Y (file)",
            "function Free (file)",
            "method Value (file) receiver",
            "method Set (file) receiver",
            "function outer (file)",
        ]
    );
}

#[test]
fn methods_are_file_scoped_with_separate_receiver_syntax() {
    let analysis = analyze_fixture("go/declarations.go");
    // A method with a receiver is NOT lexically nested inside the receiver type.
    for name in ["Value", "Set"] {
        let method = analysis
            .declarations
            .iter()
            .find(|declaration| {
                declaration.name == name && declaration.kind == DeclarationKind::Method
            })
            .expect("method");
        assert_eq!(analysis.scope_path(method.scope_id), "(file)");
        assert!(method.flags.contains(&DeclarationFlag::Receiver));
    }
    // Every receiver method is declared at file scope. Interface method
    // elements are a different construct and stay inside the interface.
    assert!(analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.flags.contains(&DeclarationFlag::Receiver))
        .all(|declaration| analysis.scope_path(declaration.scope_id) == "(file)"));
    let interface_method = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Read")
        .expect("Read");
    assert_eq!(
        analysis.scope_path(interface_method.scope_id),
        "(file)::Reader"
    );
    assert_eq!(
        reference_summary(&analysis),
        vec![
            "embedded_type Embedded",
            "embedded_type Pointer",
            "embedded_type Closer",
            "receiver_type UserID",
            "receiver_type *UserID",
        ]
    );
    // The receiver reference is attached to the method declaration it belongs to.
    let value_method = analysis
        .declarations
        .iter()
        .find(|declaration| {
            declaration.name == "Value" && declaration.kind == DeclarationKind::Method
        })
        .expect("Value method");
    let receiver = analysis
        .references
        .iter()
        .find(|reference| reference.kind == ReferenceKind::ReceiverType)
        .expect("receiver reference");
    assert_eq!(receiver.declaration_id, Some(value_method.declaration_id));
}

#[test]
fn struct_and_interface_members_are_extracted() {
    let analysis = analyze_fixture("go/declarations.go");
    let pair_scope = analysis
        .scopes
        .iter()
        .find(|scope| scope.name.as_deref() == Some("Pair"))
        .expect("Pair scope");
    assert_eq!(pair_scope.kind, ScopeKind::Struct);
    let reader_scope = analysis
        .scopes
        .iter()
        .find(|scope| scope.name.as_deref() == Some("Reader"))
        .expect("Reader scope");
    assert_eq!(reader_scope.kind, ScopeKind::Interface);
    // A generic type parameter is syntax, not a declaration.
    for name in ["T", "any"] {
        assert!(
            !analysis
                .declarations
                .iter()
                .any(|declaration| declaration.name == name),
            "`{name}` must not become a declaration"
        );
    }
    // Embedded fields have no written name, so they never become fake fields.
    assert!(!analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "Embedded" || declaration.name == "Pointer"));
}

#[test]
fn imports_cover_required_forms() {
    let analysis = analyze_fixture("go/declarations.go");
    let rendered = analysis
        .imports
        .iter()
        .map(|import| {
            let items = import
                .items
                .iter()
                .map(|item| {
                    let alias = item
                        .alias
                        .as_ref()
                        .map(|alias| format!(" as {alias}"))
                        .unwrap_or_default();
                    format!("{}{alias}[{}]", item.target, item.category.as_str())
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} items=[{items}]", import.form.as_str())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![
            "grouped items=[fmt[normal], example.com/alias as alias[normal], example.com/blank as _[blank], example.com/dot as .[dot]]",
            "single items=[example.com/single[normal]]",
        ]
    );
    assert!(analysis
        .imports
        .iter()
        .flat_map(|import| import.items.iter())
        .any(|item| item.category == ImportCategory::Blank));
    assert!(analysis
        .imports
        .iter()
        .flat_map(|import| import.items.iter())
        .any(|item| item.category == ImportCategory::Dot));
}

#[test]
fn call_forms_and_documented_ambiguities() {
    let analysis = analyze_fixture("go/calls.go");
    assert_eq!(
        call_summary(&analysis),
        vec![
            "plain_name helper dyn=false nullsafe=false",
            "member_selector obj.Method dyn=false nullsafe=false",
            "plain_name Map dyn=false nullsafe=false",
            "indirect handlers[0] dyn=false nullsafe=false",
            "plain_name getHandler dyn=false nullsafe=false",
            "indirect getHandler() dyn=false nullsafe=false",
            // `go f()` and `defer f()` are ordinary call shapes in the tree.
            "plain_name helper dyn=false nullsafe=false",
            "plain_name helper dyn=false nullsafe=false",
            // A one-argument `int64(5)` is a `call_expression` in the Go grammar,
            // so it is reported as call-shaped even though it is a conversion.
            "plain_name int64 dyn=false nullsafe=false",
        ]
    );
    let generic = analysis
        .calls
        .iter()
        .find(|call| call.callee_written == "Map")
        .expect("generic call");
    assert_eq!(generic.type_arguments.as_deref(), Some("[int, string]"));
    assert_eq!(generic.form, CallLikeForm::PlainName);
}

#[test]
fn generic_instantiation_shapes_follow_the_grammar() {
    let analysis = analyze_fixture("go/boundaries.go");
    let calls = analysis
        .calls
        .iter()
        .map(|call| {
            (
                call.callee_written.clone(),
                call.form,
                call.type_arguments.clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        calls,
        vec![
            ("Generic[int]".to_string(), CallLikeForm::Indirect, None),
            (
                "Generic".to_string(),
                CallLikeForm::PlainName,
                Some("[int, string]".to_string())
            ),
            ("int64".to_string(), CallLikeForm::PlainName, None),
            ("d.Method".to_string(), CallLikeForm::MemberSelector, None),
            (
                "d.Base.Method".to_string(),
                CallLikeForm::MemberSelector,
                None
            ),
        ]
    );
    // `Generic[int](value)` with a single call argument is a
    // `type_conversion_expression`, so it is not reported as call-like. The
    // fixture documents this; the assertion pins the behaviour.
    assert_eq!(
        analysis
            .calls
            .iter()
            .filter(|call| call.callee_written.starts_with("Generic"))
            .count(),
        2
    );
}

#[test]
fn method_expression_and_shadowing_do_not_create_declarations() {
    let analysis = analyze_fixture("go/boundaries.go");
    // `f := Base.Method` is a method expression, not a call and not a declaration.
    assert!(analysis
        .calls
        .iter()
        .all(|call| !call.callee_written.contains("Base.Method")
            || call.callee_written == "d.Base.Method"));
    // Short variable declarations are not extracted as declarations, so
    // `count` in the outer and inner block produce nothing.
    assert!(!analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "count"
            || declaration.name == "f"
            || declaration.name == "d"));
    // Build tags are not evaluated: the file is analyzed as written.
    assert_eq!(analysis.status, repodex::AnalysisStatus::Clean);
}

#[test]
fn closures_have_anonymous_scopes() {
    let analysis = analyze_fixture("go/declarations.go");
    let closures: Vec<_> = analysis
        .scopes
        .iter()
        .filter(|scope| scope.kind == ScopeKind::Closure)
        .collect();
    assert_eq!(closures.len(), 3);
    assert!(closures.iter().all(|scope| scope.name.is_none()));
    // The innermost closure is nested inside the middle closure.
    let innermost = closures
        .iter()
        .max_by_key(|scope| scope.scope_id)
        .expect("innermost closure");
    let parent = innermost.parent_scope_id.expect("parent scope");
    assert_eq!(analysis.scopes[parent as usize].kind, ScopeKind::Closure);
}

#[test]
fn test_evidence_is_syntactic_and_documented() {
    let analysis = analyze_fixture("go/sample_test.go");
    assert_eq!(
        test_evidence(&analysis, "TestSomething"),
        vec![
            "function_name_convention:Test prefix",
            "signature_shape:single *testing.T parameter"
        ]
    );
    assert_eq!(
        test_evidence(&analysis, "TestHelperOnly"),
        vec![
            "function_name_convention:Test prefix",
            "signature_shape:single *testing.T parameter"
        ]
    );
    // The Go rule is "Test" followed by a non-lowercase character, so `Test_x`
    // is a candidate and `Testify` is not.
    assert_eq!(
        test_evidence(&analysis, "Test_lowercase_after_prefix").len(),
        2
    );
    assert!(test_evidence(&analysis, "Testify").is_empty());
    assert!(test_evidence(&analysis, "helperPlain").is_empty());
    // Signature-only evidence is recorded without claiming a test.
    assert_eq!(
        test_evidence(&analysis, "helperTakingT"),
        vec!["signature_shape:single *testing.T parameter"]
    );
    assert_eq!(
        test_evidence(&analysis, "BenchmarkSomething"),
        vec![
            "function_name_convention:Benchmark prefix",
            "signature_shape:single *testing.B parameter"
        ]
    );
    // The file name is file-level evidence only.
    assert_eq!(analysis.file_test_evidence.len(), 1);
    assert_eq!(analysis.file_test_evidence[0].detail, "_test.go suffix");
    assert_eq!(analysis.test_candidate_count(), 5);
}

#[test]
fn t_run_closure_stays_a_call() {
    let analysis = analyze_fixture("go/sample_test.go");
    let t_run = analysis
        .calls
        .iter()
        .find(|call| call.callee_written == "t.Run")
        .expect("t.Run call");
    // `t.Run` is a call-shaped occurrence and never proof of test ownership.
    assert_eq!(t_run.form, CallLikeForm::MemberSelector);
    let closure_scope = analysis
        .scopes
        .iter()
        .find(|scope| scope.kind == ScopeKind::Closure)
        .expect("t.Run closure");
    assert!(closure_scope.name.is_none());
    let fatal = analysis
        .calls
        .iter()
        .find(|call| call.callee_written == "t.Fatal")
        .expect("t.Fatal call");
    assert_eq!(fatal.scope_id, closure_scope.scope_id);
}

#[test]
fn ranges_and_containment_are_source_exact() {
    let analysis = analyze_fixture("go/declarations.go");
    for declaration in &analysis.declarations {
        let name = slice(
            "go/declarations.go",
            declaration.name_range.byte_start,
            declaration.name_range.byte_end,
        );
        assert_eq!(name, declaration.name);
        let scope = &analysis.scopes[declaration.scope_id as usize];
        if scope.kind == ScopeKind::File {
            continue;
        }
        assert!(scope.range.byte_start <= declaration.range.byte_start);
        assert!(declaration.range.byte_end <= scope.range.byte_end);
    }
    let source = String::from_utf8(fixture_source("go/declarations.go")).expect("utf-8");
    assert!(source.starts_with("// Fixture: Go declarations"));
    let type_parameters = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Pair")
        .expect("Pair");
    assert_eq!(
        slice(
            "go/declarations.go",
            type_parameters.range.byte_start,
            type_parameters.range.byte_end
        ),
        "Pair[K comparable, V any] struct {\n\tKey   K\n\tValue V\n\tEmbedded\n\t*Pointer\n}"
    );
}

#[test]
fn malformed_source_reports_recovery_and_keeps_neighbours() {
    let analysis = analyze_fixture("go/malformed.go");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Recovered);
    let names: Vec<&str> = analysis
        .declarations
        .iter()
        .map(|declaration| declaration.name.as_str())
        .collect();
    assert!(names.contains(&"validBefore"));
    assert!(names.contains(&"validAfter"));
    assert!(names.contains(&"fixture"));
    // `var incomplete =` has no name written, so no variable is fabricated.
    assert!(!names.contains(&"incomplete"));
    assert!(analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.kind == repodex::DiagnosticKind::MissingSyntax));
}
