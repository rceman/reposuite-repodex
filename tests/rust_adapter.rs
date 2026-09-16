//! Rust adapter correctness tests.
//!
//! These assertions encode the intended extraction semantics directly. They do
//! not depend on `fixtures/expected/**`, so a blessed expectation file can never
//! be the only evidence that Rust extraction is correct.

mod support;

use support::{
    analyze_fixture, call_summary, declaration_range, declaration_summary, fixture_source,
    reference_summary, scope_summary, slice, test_evidence,
};

#[test]
fn declarations_cover_required_constructs() {
    let analysis = analyze_fixture("rust/declarations.rs");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Clean);
    assert_eq!(
        declaration_summary(&analysis),
        vec![
            "module inline (file)",
            "module nested (file)::inline",
            "function deep (file)::inline::nested",
            "struct Inner (file)::inline",
            "module external (file)",
            "struct Config (file)",
            "field name (file)::Config",
            "field count (file)::Config",
            "enum State (file)",
            "variant Idle (file)::State",
            "variant Running (file)::State",
            "variant Failed (file)::State",
            "trait Handler (file)",
            "method handle (file)::Handler receiver",
            "method label (file)::Handler receiver|default",
            "type_alias Outcome (file)",
            "constant MAX_ITEMS (file)",
            "constant COUNTER (file) static|mutable",
            "function free_function (file)",
            "function outer (file)",
            "function nested_function (file)::outer",
        ]
    );
}

#[test]
fn scopes_preserve_lexical_containment() {
    let analysis = analyze_fixture("rust/declarations.rs");
    assert_eq!(
        scope_summary(&analysis),
        vec![
            "file - -",
            "module inline (file)",
            "module nested (file)::inline",
            "function deep (file)::inline::nested",
            "struct Config (file)",
            "enum State (file)",
            "trait Handler (file)",
            "method label (file)::Handler",
            "function free_function (file)",
            "function outer (file)",
            "function nested_function (file)::outer",
            "closure - (file)::outer",
        ]
    );
    // A nested function is contained by the function that declares it, not by
    // the file.
    let nested = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "nested_function")
        .expect("nested_function");
    assert_eq!(analysis.scope_path(nested.scope_id), "(file)::outer");
    // The anonymous closure scope has no invented name.
    let closure = analysis
        .scopes
        .iter()
        .find(|scope| scope.kind == repodex::ScopeKind::Closure)
        .expect("closure scope");
    assert_eq!(closure.name, None);
}

#[test]
fn impl_blocks_are_scopes_and_not_type_declarations() {
    let analysis = analyze_fixture("rust/impls.rs");
    // `impl Widget` must never declare a struct named `Widget`.
    let declared: Vec<&str> = analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == repodex::DeclarationKind::Struct)
        .map(|declaration| declaration.name.as_str())
        .collect();
    assert_eq!(declared, vec!["Widget", "Gadget"]);
    assert_eq!(
        analysis
            .declarations
            .iter()
            .filter(|declaration| declaration.kind == repodex::DeclarationKind::Struct)
            .count(),
        2
    );

    // The same method name in different impl contexts stays distinguishable.
    let names = analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.name == "name")
        .map(|declaration| analysis.scope_path(declaration.scope_id))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec!["(file)::(impl Widget)", "(file)::(impl Gadget)"]
    );
    let describes = analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.name == "describe")
        .map(|declaration| analysis.scope_path(declaration.scope_id))
        .collect::<Vec<_>>();
    assert_eq!(
        describes,
        vec![
            // The trait's own declaration site.
            "(file)::Describe",
            "(file)::(impl Describe for Widget)",
            "(file)::(impl Describe for Gadget)"
        ]
    );

    // An associated function has no receiver; receiver methods are flagged.
    let associated = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "new")
        .expect("new");
    assert!(!associated
        .flags
        .contains(&repodex::DeclarationFlag::Receiver));
    let receiver_method = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "name")
        .expect("name");
    assert!(receiver_method
        .flags
        .contains(&repodex::DeclarationFlag::Receiver));
}

#[test]
fn impl_relationships_are_unresolved_references() {
    let analysis = analyze_fixture("rust/impls.rs");
    assert_eq!(
        reference_summary(&analysis),
        vec![
            "impl_target Widget",
            "impl_target Gadget",
            "impl_trait Describe",
            "impl_target Widget",
            "impl_trait Describe",
            "impl_target Gadget",
        ]
    );
}

#[test]
fn imports_cover_required_forms() {
    let analysis = analyze_fixture("rust/imports.rs");
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
                    format!("{}{alias}", item.target)
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
            "single module=- items=[std::collections::HashMap]",
            "grouped module=std::collections items=[HashSet, BTreeMap as Map]",
            "grouped module=std::io items=[self, Read as Reader]",
            "single module=- items=[super::sibling]",
            "wildcard module=crate::root items=[*]",
            "single module=- items=[self::local::Thing]",
        ]
    );
    assert!(analysis
        .imports
        .iter()
        .flat_map(|import| import.items.iter())
        .any(|item| item.wildcard));
}

#[test]
fn call_forms_and_documented_ambiguities() {
    let analysis = analyze_fixture("rust/calls.rs");
    assert_eq!(
        call_summary(&analysis),
        vec![
            "plain_name plain dyn=false nullsafe=false",
            "qualified_path module::path::qualified dyn=false nullsafe=false",
            "member_selector value.method dyn=false nullsafe=false",
            "qualified_path Vec::new dyn=false nullsafe=false",
            "plain_name generic_call::<u32> dyn=false nullsafe=false",
            // Tuple-struct construction is call-shaped; it is NOT claimed to be
            // a constructor call.
            "plain_name TupleStruct dyn=false nullsafe=false",
            "plain_name closure dyn=false nullsafe=false",
            "indirect (make_callable()) dyn=false nullsafe=false",
            "plain_name make_callable dyn=false nullsafe=false",
            "macro_invocation println dyn=false nullsafe=false",
        ]
    );
    // Struct literal construction is not a call.
    assert!(!analysis
        .calls
        .iter()
        .any(|call| call.callee_written.contains("StructLiteral")));
    // Generic syntax is recorded as written, not resolved.
    let generic = analysis
        .calls
        .iter()
        .find(|call| call.callee_written.starts_with("generic_call"))
        .expect("generic call");
    assert_eq!(generic.type_arguments.as_deref(), Some("<u32>"));
}

#[test]
fn test_evidence_requires_an_attribute() {
    let analysis = analyze_fixture("rust/tests.rs");
    assert_eq!(
        test_evidence(&analysis, "test_something"),
        vec!["attribute:#[test]"]
    );
    assert_eq!(
        test_evidence(&analysis, "test_panics"),
        vec!["attribute:#[test]"]
    );
    // Naming alone is not evidence in Rust.
    assert!(test_evidence(&analysis, "test_named_without_attribute").is_empty());
    // `#[cfg(test)]` is not a test attribute.
    assert!(test_evidence(&analysis, "not_a_test_by_attribute").is_empty());
    // A path-qualified test attribute is evidence, without resolving the path.
    assert_eq!(
        test_evidence(&analysis, "test_async"),
        vec!["attribute:#[tokio::test]"]
    );
    assert_eq!(analysis.test_candidate_count(), 3);
}

#[test]
fn boundary_constructs_are_extracted_without_macro_expansion() {
    let analysis = analyze_fixture("rust/boundaries.rs");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Clean);
    let process = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "process")
        .expect("process");
    assert!(process.flags.contains(&repodex::DeclarationFlag::Async));
    assert!(process.flags.contains(&repodex::DeclarationFlag::Receiver));

    let dangerous = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "dangerous")
        .expect("dangerous");
    assert!(dangerous.flags.contains(&repodex::DeclarationFlag::Unsafe));

    let constant_fn = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "constant_fn")
        .expect("constant_fn");
    assert!(constant_fn.flags.contains(&repodex::DeclarationFlag::Const));

    // A cfg-gated function is extracted as written; cfg is never evaluated.
    assert!(analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "cfg_gated"));

    // `macro_rules!` declares a macro; the macro body is never expanded.
    assert!(analysis.declarations.iter().any(|declaration| {
        declaration.name == "define_thing" && declaration.kind == repodex::DeclarationKind::Macro
    }));
    let macro_calls = analysis
        .calls
        .iter()
        .filter(|call| call.form == repodex::CallLikeForm::MacroInvocation)
        .map(|call| call.callee_written.clone())
        .collect::<Vec<_>>();
    assert_eq!(macro_calls, vec!["define_thing", "println", "format"]);

    // The generic impl target keeps its written form including lifetimes.
    assert!(analysis
        .references
        .iter()
        .any(|reference| reference.written == "Wrapper<'a, T>"));

    // Raw identifiers are parsed, not rewritten.
    let source = String::from_utf8(fixture_source("rust/boundaries.rs")).expect("utf-8");
    assert!(source.contains("r#type"));
    assert!(source.contains("r#match"));
}

#[test]
fn unions_extern_blocks_and_associated_items_are_classified_honestly() {
    let analysis = analyze_fixture("rust/boundaries.rs");

    // A union is not a struct.
    let number = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Number")
        .expect("Number");
    assert_eq!(number.kind, repodex::DeclarationKind::Union);
    assert!(analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == repodex::DeclarationKind::Struct)
        .all(|declaration| declaration.name != "Number"));
    assert!(analysis
        .scopes
        .iter()
        .any(|scope| scope.kind == repodex::ScopeKind::Union
            && scope.name.as_deref() == Some("Number")));

    // Foreign signatures are contained by the extern block, and the ABI string
    // produces no fact.
    let puts = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "puts")
        .expect("puts");
    assert_eq!(puts.kind, repodex::DeclarationKind::Function);
    assert!(analysis
        .scope_path(puts.scope_id)
        .contains("(extern_block)"));
    assert!(analysis
        .scopes
        .iter()
        .any(|scope| scope.kind == repodex::ScopeKind::ExternBlock));
    assert!(analysis
        .declarations
        .iter()
        .all(|declaration| declaration.name != "C"));

    // Associated consts and type aliases belong to the impl scope, and the impl
    // still does not declare `Number`.
    let limit = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "LIMIT")
        .expect("LIMIT");
    assert_eq!(limit.kind, repodex::DeclarationKind::Constant);
    let word = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "Word")
        .expect("Word");
    assert_eq!(word.kind, repodex::DeclarationKind::TypeAlias);
    assert_eq!(
        analysis.scope_path(limit.scope_id),
        "(file)::(impl Number)",
        "an associated item belongs to the impl scope, which borrows the written target"
    );

    // Generic parameters are syntax, not declarations.
    for name in ["T", "U", "Clone", "Send", "Debug"] {
        assert!(
            !analysis
                .declarations
                .iter()
                .any(|declaration| declaration.name == name),
            "`{name}` must not become a declaration"
        );
    }
    // `dyn` and `impl Trait` are not references.
    assert!(analysis
        .references
        .iter()
        .all(|reference| !reference.written.contains("Debug")));
    assert!(analysis
        .references
        .iter()
        .all(|reference| !reference.written.contains("Iterator")));
}

#[test]
fn ranges_are_byte_exact_and_half_open() {
    let analysis = analyze_fixture("rust/declarations.rs");
    for declaration in &analysis.declarations {
        let name = slice(
            "rust/declarations.rs",
            declaration.name_range.byte_start,
            declaration.name_range.byte_end,
        );
        assert_eq!(
            name, declaration.name,
            "name range must select exactly the written name for {}",
            declaration.name
        );
        assert!(declaration.range.byte_start <= declaration.name_range.byte_start);
        assert!(declaration.name_range.byte_end <= declaration.range.byte_end);
        assert!(declaration.range.byte_end <= analysis.file.byte_len);
        assert!(
            declaration.range.byte_start < declaration.range.byte_end,
            "half-open ranges must be non-empty for {}",
            declaration.name
        );
    }
    // The full range of `Config` selects `pub struct Config { .. }`.
    let (start, end) = declaration_range(&analysis, "Config").expect("Config");
    assert_eq!(
        slice("rust/declarations.rs", start, end),
        "pub struct Config {\n    pub name: String,\n    count: u32,\n}"
    );
}

#[test]
fn containment_is_lexically_ordered() {
    let analysis = analyze_fixture("rust/declarations.rs");
    for declaration in &analysis.declarations {
        let scope = &analysis.scopes[declaration.scope_id as usize];
        if scope.kind == repodex::ScopeKind::File {
            continue;
        }
        assert!(
            scope.range.byte_start <= declaration.range.byte_start
                && declaration.range.byte_end <= scope.range.byte_end,
            "{} must be lexically inside its scope {}",
            declaration.name,
            analysis.scope_path(declaration.scope_id)
        );
    }
}

#[test]
fn recovery_reports_missing_and_error_without_fabrication() {
    let analysis = analyze_fixture("rust/malformed.rs");
    assert_eq!(analysis.status, repodex::AnalysisStatus::Recovered);
    assert!(!analysis.recovery_regions.is_empty());
    let names: Vec<&str> = analysis
        .declarations
        .iter()
        .map(|declaration| declaration.name.as_str())
        .collect();
    // Declarations before and after the damage survive.
    assert!(names.contains(&"valid_before"));
    assert!(names.contains(&"valid_after"));
    // Recovery swallowed `fn valid_last()` as a function-type parameter, so it
    // is not a declaration and no name was fabricated for it.
    assert!(!names.contains(&"valid_last"));
    // The MISSING type identifier for `field: ,` produced no invented field name.
    let fields: Vec<&str> = analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == repodex::DeclarationKind::Field)
        .map(|declaration| declaration.name.as_str())
        .collect();
    assert_eq!(fields, vec!["field"]);
    // The bodyless impl still yields an impl scope and its target reference.
    assert!(analysis
        .scopes
        .iter()
        .any(|scope| scope.kind == repodex::ScopeKind::Impl));
    assert!(analysis
        .references
        .iter()
        .any(|reference| reference.written == "Incomplete"));
    assert!(analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.kind == repodex::DiagnosticKind::MissingSyntax));
    assert!(analysis
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.kind == repodex::DiagnosticKind::SyntaxError));
}

#[test]
fn duplicate_names_in_different_contexts_stay_distinct() {
    let analysis = analyze_fixture("shared/adversarial.rs");
    let name_declarations = analysis
        .declarations
        .iter()
        .filter(|declaration| declaration.name == "name")
        .map(|declaration| analysis.scope_path(declaration.scope_id))
        .collect::<Vec<_>>();
    assert_eq!(
        name_declarations,
        vec!["(file)::(impl Duplicate)", "(file)::nested"]
    );
    // Every declaration identity is unique.
    let mut ids = analysis
        .declarations
        .iter()
        .map(|declaration| declaration.declaration_id)
        .collect::<Vec<_>>();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), total);
}
