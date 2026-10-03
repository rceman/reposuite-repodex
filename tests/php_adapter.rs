//! PHP adapter correctness tests.

mod support;

use repodex::{
    CallLikeForm, DeclarationFlag, DeclarationKind, ImportCategory, LanguageId, ReferenceKind,
    ScopeKind,
};
use support::{
    analyze_fixture, analyze_source, call_summary, declaration_summary, fixture, reference_summary,
    slice, test_evidence,
};

#[test]
fn declarations_cover_required_constructs() {
    let analysis = analyze_fixture("php/declarations.php");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Clean);
    assert_eq!(
        declaration_summary(&analysis),
        vec![
            "namespace App\\Service (file)",
            "interface HandlerInterface (file)::App\\Service",
            "constant VERSION (file)::App\\Service::HandlerInterface",
            "method handle (file)::App\\Service::HandlerInterface",
            "trait Loggable (file)::App\\Service",
            "method log (file)::App\\Service::Loggable",
            "class BaseService (file)::App\\Service abstract",
            "constant DEFAULT_NAME (file)::App\\Service::BaseService",
            "property cacheKey (file)::App\\Service::BaseService protected|static",
            "property version (file)::App\\Service::BaseService readonly",
            "method __construct (file)::App\\Service::BaseService constructor",
            "property name (file)::App\\Service::BaseService promoted",
            "property limit (file)::App\\Service::BaseService promoted",
            "method handle (file)::App\\Service::BaseService abstract",
            "class Service (file)::App\\Service final",
            "method make (file)::App\\Service::Service static",
            "method handle (file)::App\\Service::Service",
            "enum Status (file)::App\\Service",
            "variant Active (file)::App\\Service::Status",
            "variant Closed (file)::App\\Service::Status",
            "method label (file)::App\\Service::Status",
            "function helperFunction (file)::App\\Service",
        ]
    );
}

#[test]
fn namespace_imports_and_trait_composition_are_distinct() {
    let analysis = analyze_fixture("php/declarations.php");
    // Five `use` statements at namespace level.
    assert_eq!(analysis.imports.len(), 5);
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
            format!(
                "{} module={} items=[{items}]",
                import.form.as_str(),
                import.module.as_deref().unwrap_or("-")
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![
            "single module=- items=[App\\Contracts\\Handler[normal]]",
            "single module=- items=[App\\Support\\Logger as Log[normal]]",
            "grouped module=App\\Support items=[Cache[normal], Clock as C[normal]]",
            "single module=- items=[App\\Support\\helper[function]]",
            "single module=- items=[App\\Support\\VERSION[constant]]",
        ]
    );
    assert!(analysis
        .imports
        .iter()
        .flat_map(|import| import.items.iter())
        .any(|item| item.category == ImportCategory::Function));
    assert!(analysis
        .imports
        .iter()
        .flat_map(|import| import.items.iter())
        .any(|item| item.category == ImportCategory::Constant));

    // `use Loggable;` inside the class body is trait composition, not an import.
    assert!(analysis.references.iter().any(|reference| reference.kind
        == ReferenceKind::TraitComposition
        && reference.written == "Loggable"));
    assert!(!analysis
        .imports
        .iter()
        .any(|import| import.items.iter().any(|item| item.target == "Loggable")));
}

#[test]
fn promoted_properties_belong_to_the_class_scope() {
    let analysis = analyze_fixture("php/declarations.php");
    for name in ["name", "limit"] {
        let promoted = analysis
            .declarations
            .iter()
            .find(|declaration| {
                declaration.name == name && declaration.kind == DeclarationKind::Property
            })
            .expect("promoted property");
        assert!(promoted.flags.contains(&DeclarationFlag::Promoted));
        assert_eq!(
            analysis.scope_path(promoted.scope_id),
            "(file)::App\\Service::BaseService"
        );
    }
    // The `$` sigil is not part of the recorded name.
    assert!(!analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name.starts_with('$')));
}

#[test]
fn class_relationships_are_unresolved_references() {
    let analysis = analyze_fixture("php/declarations.php");
    assert_eq!(
        reference_summary(&analysis),
        vec![
            "implemented_interface HandlerInterface",
            "trait_composition Loggable",
            "base_class BaseService",
        ]
    );
}

#[test]
fn call_forms_cover_required_shapes() {
    let analysis = analyze_fixture("php/calls.php");
    assert_eq!(
        call_summary(&analysis),
        vec![
            "explicit_construction self dyn=false nullsafe=false",
            "plain_name plainCall dyn=false nullsafe=false",
            "member_selector $widget?->render dyn=false nullsafe=true",
            "member_selector $widget->render dyn=false nullsafe=false",
            "static_scoped Widget::create dyn=false nullsafe=false",
            "static_scoped \\App\\Calls\\Widget::create dyn=false nullsafe=false",
            "indirect $callable dyn=true nullsafe=false",
            "member_selector $widget->{$name} dyn=true nullsafe=false",
            "explicit_construction Widget dyn=false nullsafe=false",
            "plain_name helperWithDefault dyn=false nullsafe=false",
            "indirect strlen(...) dyn=true nullsafe=false",
            "explicit_construction class dyn=true nullsafe=false",
            "explicit_construction class dyn=true nullsafe=false",
        ]
    );
    let nullsafe = analysis
        .calls
        .iter()
        .find(|call| call.nullsafe)
        .expect("nullsafe call");
    assert_eq!(nullsafe.form, CallLikeForm::MemberSelector);
    // Dynamic callees are flagged rather than resolved.
    assert_eq!(
        analysis
            .calls
            .iter()
            .filter(|call| call.dynamic_callee)
            .count(),
        5
    );
}

/// An anonymous class has no name, so the written callee is the `class` keyword
/// alone. The `anonymous_class` node spans from any leading attribute list
/// through the whole `class ... { ... }` body, so a callee range taken from
/// that node would make a five-byte name claim the entire declaration. The
/// range must cover exactly the keyword, with and without attributes.
#[test]
fn anonymous_class_construction_ranges_only_the_keyword() {
    let source = std::fs::read(fixture("php/calls.php")).expect("fixture must be readable");
    let analysis = analyze_source(LanguageId::Php, "calls.php", &source);
    let anonymous: Vec<_> = analysis
        .calls
        .iter()
        .filter(|call| call.callee_written == "class")
        .collect();
    // One plain and one attributed anonymous class, so neither path can regress
    // unnoticed.
    assert_eq!(
        anonymous.len(),
        2,
        "the fixture builds two anonymous classes"
    );
    for call in anonymous {
        assert_eq!(call.form, CallLikeForm::ExplicitConstruction);
        let start = call.callee_range.byte_start as usize;
        let end = call.callee_range.byte_end as usize;
        assert_eq!(
            end - start,
            "class".len(),
            "the callee range must be exactly the keyword"
        );
        assert_eq!(&source[start..end], b"class");
        // The expression really does span the class body, so the two ranges are
        // genuinely different and this test is not vacuous.
        let expr_start = call.expression_range.byte_start as usize;
        let expr_end = call.expression_range.byte_end as usize;
        assert!(
            expr_end - expr_start > 40,
            "the construction expression must span the anonymous class body"
        );
    }
}

#[test]
fn first_class_callable_is_not_an_invocation() {
    let analysis = analyze_fixture("php/calls.php");
    let first_class: Vec<&str> = analysis
        .references
        .iter()
        .filter(|reference| reference.kind == ReferenceKind::FirstClassCallable)
        .map(|reference| reference.written.as_str())
        .collect();
    assert_eq!(first_class, vec!["strlen", "strlen"]);
    // `strlen(...)` alone is not a call; `strlen(...)($callable)` is.
    let strlen_calls = analysis
        .calls
        .iter()
        .filter(|call| call.callee_written.starts_with("strlen"))
        .count();
    assert_eq!(strlen_calls, 1);
}

#[test]
fn closures_arrow_functions_and_captures() {
    let analysis = analyze_fixture("php/calls.php");
    let closure = analysis
        .scopes
        .iter()
        .find(|scope| scope.kind == ScopeKind::Closure)
        .expect("closure scope");
    assert!(closure.name.is_none());
    let arrow = analysis
        .scopes
        .iter()
        .find(|scope| scope.kind == ScopeKind::ArrowFunction)
        .expect("arrow function scope");
    assert!(arrow.name.is_none());
    assert!(analysis
        .references
        .iter()
        .any(|reference| reference.kind == ReferenceKind::ClosureCapture
            && reference.written == "$firstClass"));
    assert_eq!(
        analysis
            .references
            .iter()
            .filter(|reference| reference.kind == ReferenceKind::ClosureCapture)
            .count(),
        1
    );
}

#[test]
fn phpunit_style_test_evidence() {
    let analysis = analyze_fixture("php/tests.php");
    assert_eq!(
        test_evidence(&analysis, "testSomething"),
        vec!["function_name_convention:test prefix"]
    );
    assert_eq!(
        test_evidence(&analysis, "annotatedMethod"),
        vec!["attribute:#[Test]"]
    );
    assert!(test_evidence(&analysis, "helperMethod").is_empty());
    // The `test` prefix applies in any class; no claim is made about PHPUnit.
    assert_eq!(
        test_evidence(&analysis, "testLooking"),
        vec!["function_name_convention:test prefix"]
    );
    // The base clause is recorded but is not used as PHP test evidence.
    assert!(test_evidence(&analysis, "ServiceTest").is_empty());
    assert_eq!(analysis.test_candidate_count(), 3);
}

#[test]
fn mixed_html_and_multiple_php_regions() {
    let analysis = analyze_fixture("php/boundaries.php");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Clean);
    let namespaces: Vec<&str> = analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == DeclarationKind::Namespace)
        .map(|declaration| declaration.name.as_str())
        .collect();
    assert_eq!(
        namespaces,
        vec!["App\\Boundaries", "App\\Boundaries\\Second"]
    );
    // Declarations from the second `<?php` region are attributed to its namespace.
    let second = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "inSecondRegion")
        .expect("inSecondRegion");
    assert_eq!(
        analysis.scope_path(second.scope_id),
        "(file)::App\\Boundaries\\Second"
    );
    // `include` / `require_once` targets are unresolved references.
    assert_eq!(
        analysis
            .references
            .iter()
            .filter(|reference| reference.kind == ReferenceKind::IncludeTarget)
            .count(),
        2
    );
}

#[test]
fn trait_adaptations_and_dynamic_members() {
    let analysis = analyze_fixture("php/boundaries.php");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Clean);
    let composed: Vec<&str> = analysis
        .references
        .iter()
        .filter(|reference| reference.kind == ReferenceKind::TraitComposition)
        .map(|reference| reference.written.as_str())
        .collect();
    assert_eq!(composed, vec!["First", "Second"]);
    let dynamic = analysis
        .calls
        .iter()
        .find(|call| call.callee_written == "$this->$method")
        .expect("dynamic member call");
    assert!(dynamic.dynamic_callee);
    assert_eq!(dynamic.form, CallLikeForm::MemberSelector);
    // Two traits declaring the same method name stay distinct by scope.
    let shared: Vec<String> = analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.name == "shared")
        .map(|declaration| analysis.scope_path(declaration.scope_id))
        .collect();
    assert_eq!(
        shared,
        vec![
            "(file)::App\\Boundaries::First",
            "(file)::App\\Boundaries::Second"
        ]
    );
}

#[test]
fn ranges_and_containment_are_source_exact() {
    let analysis = analyze_fixture("php/declarations.php");
    for declaration in &analysis.declarations {
        if declaration.kind == DeclarationKind::Namespace {
            // The written namespace name includes the backslash separators.
            continue;
        }
        let name = slice(
            "php/declarations.php",
            declaration.name_range.byte_start,
            declaration.name_range.byte_end,
        );
        assert_eq!(name, declaration.name, "for {declaration:?}");
    }
    let namespace = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.kind == DeclarationKind::Namespace)
        .expect("namespace");
    assert_eq!(
        slice(
            "php/declarations.php",
            namespace.name_range.byte_start,
            namespace.name_range.byte_end
        ),
        "App\\Service"
    );
}

#[test]
fn malformed_source_reports_recovery_and_keeps_neighbours() {
    let analysis = analyze_fixture("php/malformed.php");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Recovered);
    let names: Vec<&str> = analysis
        .declarations
        .iter()
        .map(|declaration| declaration.name.as_str())
        .collect();
    assert!(names.contains(&"validBefore"));
    assert!(names.contains(&"validAfter"));
    assert!(names.contains(&"Broken"));
    // The unterminated `function incomplete(` declares nothing.
    assert!(!names.contains(&"incomplete"));
    assert!(analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.kind == repodex::DiagnosticKind::MissingSyntax));
}

#[test]
fn scoped_call_dynamic_member_name_is_flagged() {
    // §21 regression: `Foo::{$m}()` / `Foo::$m()` have a literal scope but a
    // dynamic selector. Before the fix only the scope's literalness was
    // checked, so these were marked non-dynamic and could feed candidates.
    let source = br#"<?php
namespace A;
function f(string $m): void {
    Helper::{$m}();
    Helper::$m();
    Helper::literal();
}
"#;
    let analysis = analyze_source(LanguageId::Php, "dyn_scope.php", source);
    assert_eq!(analysis.status, repodex::AnalysisStatus::Clean);
    let get = |w: &str| {
        analysis
            .calls
            .iter()
            .find(|c| c.callee_written == w)
            .unwrap_or_else(|| panic!("missing call {w}"))
    };
    assert!(get("Helper::{$m}").dynamic_callee);
    assert!(get("Helper::$m").dynamic_callee);
    assert!(!get("Helper::literal").dynamic_callee);
}
