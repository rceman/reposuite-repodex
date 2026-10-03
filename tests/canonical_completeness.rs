//! Regression tests for the two comparison mechanisms the TASK 1 evidence
//! rests on.
//!
//! * The canonical text is what the digests, the determinism tests and the
//!   incremental harness compare. It must render **every** normalized field, or
//!   two different analyses would compare equal.
//! * `FileAnalysis::same_facts` is the authoritative structural comparison and
//!   must agree with the canonical text.
//!
//! These tests are exhaustive by construction: they walk every fact of every
//! committed fixture and mutate every field of every fact in turn, asserting
//! that the mutation is visible both structurally and in the canonical text.
//! A field that is missing from the canonical rendering makes this file fail.

mod support;

use repodex::model::{DiagnosticSeverity, FileAnalysis, SourceRange, TestEvidence};
use repodex::{
    AnalysisStatus, CallLikeForm, DeclarationFlag, DeclarationKind, DiagnosticKind, ImportCategory,
    ImportForm, LanguageId, ReferenceKind, ScopeKind, TestEvidenceKind,
};

/// A value guaranteed to differ from `current`.
///
/// Every mutation must actually change something, otherwise the sweep would
/// report a false "invisible field" for a mutation that was a no-op.
fn different<T: Copy + PartialEq>(current: T, candidates: &[T]) -> T {
    *candidates
        .iter()
        .find(|candidate| **candidate != current)
        .expect("at least two variants")
}

fn different_string(current: &str) -> String {
    let mut candidate = format!("{current}-mutated");
    while candidate == current {
        candidate.push('x');
    }
    candidate
}

/// A scope id no real scope can have.
fn foreign_id() -> u32 {
    u32::MAX - 1
}

/// A range guaranteed to differ from `current`, including when it is absent.
fn different_range(current: Option<SourceRange>) -> SourceRange {
    match current {
        Some(range) => shift_points(range),
        None => SourceRange::new(0, 0, 11, 12, 13, 14),
    }
}

/// Move only the row/column coordinates, leaving the bytes untouched.
///
/// This is the case the previous canonical rendering could not see at all.
fn shift_points(range: SourceRange) -> SourceRange {
    SourceRange::new(
        range.byte_start,
        range.byte_end,
        range.row_start + 1,
        range.column_start,
        range.row_end + 1,
        range.column_end,
    )
}

/// Move only the columns, leaving bytes and rows untouched.
fn shift_columns(range: SourceRange) -> SourceRange {
    SourceRange::new(
        range.byte_start,
        range.byte_end,
        range.row_start,
        range.column_start + 1,
        range.row_end,
        range.column_end + 1,
    )
}

const LANGUAGES: [LanguageId; 4] = [
    LanguageId::Rust,
    LanguageId::Go,
    LanguageId::Python,
    LanguageId::Php,
];

const SCOPE_KINDS: [ScopeKind; 16] = [
    ScopeKind::File,
    ScopeKind::Module,
    ScopeKind::Package,
    ScopeKind::Namespace,
    ScopeKind::Struct,
    ScopeKind::Union,
    ScopeKind::Class,
    ScopeKind::Interface,
    ScopeKind::Trait,
    ScopeKind::Enum,
    ScopeKind::Impl,
    ScopeKind::ExternBlock,
    ScopeKind::Function,
    ScopeKind::Method,
    ScopeKind::Closure,
    ScopeKind::Lambda,
];

const DECLARATION_KINDS: [DeclarationKind; 18] = [
    DeclarationKind::Module,
    DeclarationKind::Package,
    DeclarationKind::Namespace,
    DeclarationKind::Function,
    DeclarationKind::Method,
    DeclarationKind::Struct,
    DeclarationKind::Union,
    DeclarationKind::Class,
    DeclarationKind::Interface,
    DeclarationKind::Trait,
    DeclarationKind::Enum,
    DeclarationKind::TypeAlias,
    DeclarationKind::NamedType,
    DeclarationKind::Constant,
    DeclarationKind::Variable,
    DeclarationKind::Field,
    DeclarationKind::Property,
    DeclarationKind::Variant,
];

const IMPORT_FORMS: [ImportForm; 3] = [
    ImportForm::Single,
    ImportForm::Grouped,
    ImportForm::Wildcard,
];

const IMPORT_CATEGORIES: [ImportCategory; 5] = [
    ImportCategory::Normal,
    ImportCategory::Function,
    ImportCategory::Constant,
    ImportCategory::Blank,
    ImportCategory::Dot,
];

const REFERENCE_KINDS: [ReferenceKind; 11] = [
    ReferenceKind::ImplTarget,
    ReferenceKind::ImplTrait,
    ReferenceKind::ReceiverType,
    ReferenceKind::BaseClass,
    ReferenceKind::ImplementedInterface,
    ReferenceKind::TraitComposition,
    ReferenceKind::EmbeddedType,
    ReferenceKind::ClosureCapture,
    ReferenceKind::IncludeTarget,
    ReferenceKind::FirstClassCallable,
    ReferenceKind::Decorator,
];

const CALL_FORMS: [CallLikeForm; 8] = [
    CallLikeForm::PlainName,
    CallLikeForm::QualifiedPath,
    CallLikeForm::MemberSelector,
    CallLikeForm::StaticScoped,
    CallLikeForm::Indirect,
    CallLikeForm::TypeConversion,
    CallLikeForm::ExplicitConstruction,
    CallLikeForm::MacroInvocation,
];

const TEST_EVIDENCE_KINDS: [TestEvidenceKind; 7] = [
    TestEvidenceKind::Attribute,
    TestEvidenceKind::Decorator,
    TestEvidenceKind::FunctionNameConvention,
    TestEvidenceKind::ClassNameConvention,
    TestEvidenceKind::FileNameConvention,
    TestEvidenceKind::ClassBaseSyntax,
    TestEvidenceKind::SignatureShape,
];

const DIAGNOSTIC_KINDS: [DiagnosticKind; 12] = [
    DiagnosticKind::SyntaxError,
    DiagnosticKind::MissingSyntax,
    DiagnosticKind::RecoveryRegion,
    DiagnosticKind::UnsupportedLanguage,
    DiagnosticKind::UnsupportedEncoding,
    DiagnosticKind::FileTooLarge,
    DiagnosticKind::IoError,
    DiagnosticKind::DisappearedDuringScan,
    DiagnosticKind::QueryError,
    DiagnosticKind::QueryMatchLimitExceeded,
    DiagnosticKind::TraversalFailure,
    DiagnosticKind::GrammarLoadError,
];

const RECEIVER_EVIDENCE_KINDS: [repodex::model::ReceiverEvidenceKind; 6] = [
    repodex::model::ReceiverEvidenceKind::ParameterTypeHint,
    repodex::model::ReceiverEvidenceKind::PropertyTypeHint,
    repodex::model::ReceiverEvidenceKind::LocalLiteralNew,
    repodex::model::ReceiverEvidenceKind::PropertyLiteralNew,
    repodex::model::ReceiverEvidenceKind::LocalOpaqueWrite,
    repodex::model::ReceiverEvidenceKind::PropertyOpaqueWrite,
];

const DIAGNOSTIC_SEVERITIES: [DiagnosticSeverity; 3] = [
    DiagnosticSeverity::Info,
    DiagnosticSeverity::Warning,
    DiagnosticSeverity::Error,
];

const DECLARATION_FLAGS: [DeclarationFlag; 15] = [
    DeclarationFlag::Async,
    DeclarationFlag::Static,
    DeclarationFlag::Abstract,
    DeclarationFlag::Final,
    DeclarationFlag::Readonly,
    DeclarationFlag::Const,
    DeclarationFlag::Default,
    DeclarationFlag::Receiver,
    DeclarationFlag::Constructor,
    DeclarationFlag::Promoted,
    DeclarationFlag::Mutable,
    DeclarationFlag::Unsafe,
    DeclarationFlag::Variadic,
    DeclarationFlag::Private,
    DeclarationFlag::Protected,
];

/// Every mutation applied to one analysis, with a label.
fn mutations(analysis: &FileAnalysis) -> Vec<(String, FileAnalysis)> {
    let mut out = Vec::new();

    // --- SourceFile -------------------------------------------------------
    let mut file = analysis.clone();
    file.file.relative_path = different_string(&analysis.file.relative_path);
    out.push(("file.relative_path".to_string(), file));

    let mut file = analysis.clone();
    file.file.language = different(analysis.file.language, &LANGUAGES);
    out.push(("file.language".to_string(), file));

    let mut file = analysis.clone();
    file.file.byte_len += 1;
    out.push(("file.byte_len".to_string(), file));

    let mut file = analysis.clone();
    file.file.line_count += 1;
    out.push(("file.line_count".to_string(), file));

    let mut file = analysis.clone();
    file.file.has_final_newline = !file.file.has_final_newline;
    out.push(("file.has_final_newline".to_string(), file));

    let mut file = analysis.clone();
    file.file.snapshot_id = different_string(&analysis.file.snapshot_id);
    out.push(("file.snapshot_id".to_string(), file));

    let mut file = analysis.clone();
    file.schema_version += 1;
    out.push(("schema_version".to_string(), file));

    let mut file = analysis.clone();
    file.status = different(
        analysis.status,
        &[
            AnalysisStatus::Clean,
            AnalysisStatus::Recovered,
            AnalysisStatus::Incomplete,
            AnalysisStatus::Unsupported,
            AnalysisStatus::Failed,
        ],
    );
    out.push(("status".to_string(), file));

    // --- Scopes -----------------------------------------------------------
    for (index, scope) in analysis.scopes.iter().enumerate() {
        let label = format!("scopes[{index}]");

        let mut clone = analysis.clone();
        clone.scopes[index].kind = different(scope.kind, &SCOPE_KINDS);
        out.push((format!("{label}.kind"), clone));

        let mut clone = analysis.clone();
        clone.scopes[index].name = Some(match scope.name.as_deref() {
            Some(name) => different_string(name),
            None => "mutated_scope".to_string(),
        });
        out.push((format!("{label}.name"), clone));

        let mut clone = analysis.clone();
        clone.scopes[index].parent_scope_id = Some(foreign_id());
        out.push((format!("{label}.parent_scope_id"), clone));

        let mut clone = analysis.clone();
        clone.scopes[index].range = shift_points(scope.range);
        out.push((format!("{label}.range row/column"), clone));

        let mut clone = analysis.clone();
        clone.scopes[index].range = shift_columns(scope.range);
        out.push((format!("{label}.range column"), clone));

        let mut clone = analysis.clone();
        clone.scopes[index].header_range = Some(different_range(scope.header_range));
        out.push((format!("{label}.header_range"), clone));

        let mut clone = analysis.clone();
        clone.scopes[index].language = different(scope.language, &LANGUAGES);
        out.push((format!("{label}.language"), clone));
    }

    // --- Declarations -----------------------------------------------------
    for (index, declaration) in analysis.declarations.iter().enumerate() {
        let label = format!("declarations[{index}]");

        let mut clone = analysis.clone();
        clone.declarations[index].kind = different(declaration.kind, &DECLARATION_KINDS);
        out.push((format!("{label}.kind"), clone));

        let mut clone = analysis.clone();
        clone.declarations[index].name = different_string(&declaration.name);
        out.push((format!("{label}.name"), clone));

        let mut clone = analysis.clone();
        clone.declarations[index].scope_id = foreign_id();
        out.push((format!("{label}.scope_id"), clone));

        // `flags` must differ as a whole vector.
        let mut clone = analysis.clone();
        let replacement = different(
            declaration.flags.first().copied(),
            &DECLARATION_FLAGS.map(Some),
        );
        clone.declarations[index].flags = vec![replacement.expect("a different flag")];
        out.push((format!("{label}.flags"), clone));

        let mut clone = analysis.clone();
        clone.declarations[index].range = shift_points(declaration.range);
        out.push((format!("{label}.range row/column"), clone));

        let mut clone = analysis.clone();
        clone.declarations[index].name_range = shift_points(declaration.name_range);
        out.push((format!("{label}.name_range row/column"), clone));

        let mut clone = analysis.clone();
        clone.declarations[index].body_range = Some(different_range(declaration.body_range));
        out.push((format!("{label}.body_range"), clone));

        // TestEvidence is nested inside a declaration: kind, detail and range.
        if let Some(evidence) = declaration.test_evidence.first() {
            let mut clone = analysis.clone();
            clone.declarations[index].test_evidence[0].kind =
                different(evidence.kind, &TEST_EVIDENCE_KINDS);
            out.push((format!("{label}.test_evidence.kind"), clone));

            let mut clone = analysis.clone();
            clone.declarations[index].test_evidence[0].detail = different_string(&evidence.detail);
            out.push((format!("{label}.test_evidence.detail"), clone));

            let mut clone = analysis.clone();
            clone.declarations[index].test_evidence[0].range = shift_points(evidence.range);
            out.push((format!("{label}.test_evidence.range row/column"), clone));
        }

        let mut clone = analysis.clone();
        clone.declarations[index].snapshot_id = different_string(&declaration.snapshot_id);
        out.push((format!("{label}.snapshot_id"), clone));

        let mut clone = analysis.clone();
        clone.declarations[index].relative_path = different_string(&declaration.relative_path);
        out.push((format!("{label}.relative_path"), clone));

        let mut clone = analysis.clone();
        clone.declarations[index].language = different(declaration.language, &LANGUAGES);
        out.push((format!("{label}.language"), clone));
    }

    // --- Imports ----------------------------------------------------------
    for (index, import) in analysis.imports.iter().enumerate() {
        let label = format!("imports[{index}]");

        let mut clone = analysis.clone();
        clone.imports[index].form = different(import.form, &IMPORT_FORMS);
        out.push((format!("{label}.form"), clone));

        let mut clone = analysis.clone();
        clone.imports[index].module = Some(match import.module.as_deref() {
            Some(module) => different_string(module),
            None => "mutated::module".to_string(),
        });
        out.push((format!("{label}.module"), clone));

        let mut clone = analysis.clone();
        clone.imports[index].module_range = Some(different_range(import.module_range));
        out.push((format!("{label}.module_range"), clone));

        let mut clone = analysis.clone();
        clone.imports[index].relative_levels = match import.relative_levels {
            Some(levels) => Some(levels + 1),
            None => Some(2),
        };
        out.push((format!("{label}.relative_levels"), clone));

        let mut clone = analysis.clone();
        clone.imports[index].scope_id = foreign_id();
        out.push((format!("{label}.scope_id"), clone));

        let mut clone = analysis.clone();
        clone.imports[index].statement_range = shift_points(import.statement_range);
        out.push((format!("{label}.statement_range row/column"), clone));

        // `statement_text` was missing from the previous canonical rendering.
        let mut clone = analysis.clone();
        clone.imports[index].statement_text = different_string(&import.statement_text);
        out.push((format!("{label}.statement_text"), clone));

        let mut clone = analysis.clone();
        clone.imports[index].snapshot_id = different_string(&import.snapshot_id);
        out.push((format!("{label}.snapshot_id"), clone));

        let mut clone = analysis.clone();
        clone.imports[index].relative_path = different_string(&import.relative_path);
        out.push((format!("{label}.relative_path"), clone));

        let mut clone = analysis.clone();
        clone.imports[index].language = different(import.language, &LANGUAGES);
        out.push((format!("{label}.language"), clone));

        for (item_index, item) in import.items.iter().enumerate() {
            let item_label = format!("{label}.items[{item_index}]");

            let mut clone = analysis.clone();
            clone.imports[index].items[item_index].target = different_string(&item.target);
            out.push((format!("{item_label}.target"), clone));

            // The item's own ranges: missing from the previous rendering.
            let mut clone = analysis.clone();
            clone.imports[index].items[item_index].target_range = shift_points(item.target_range);
            out.push((format!("{item_label}.target_range row/column"), clone));

            let mut clone = analysis.clone();
            clone.imports[index].items[item_index].target_range = shift_columns(item.target_range);
            out.push((format!("{item_label}.target_range column"), clone));

            let mut clone = analysis.clone();
            clone.imports[index].items[item_index].alias = Some(match item.alias.as_deref() {
                Some(alias) => different_string(alias),
                None => "mutated_alias".to_string(),
            });
            out.push((format!("{item_label}.alias"), clone));

            let mut clone = analysis.clone();
            clone.imports[index].items[item_index].alias_range =
                Some(different_range(item.alias_range));
            out.push((format!("{item_label}.alias_range"), clone));

            let mut clone = analysis.clone();
            clone.imports[index].items[item_index].wildcard = !item.wildcard;
            out.push((format!("{item_label}.wildcard"), clone));

            let mut clone = analysis.clone();
            clone.imports[index].items[item_index].category =
                different(item.category, &IMPORT_CATEGORIES);
            out.push((format!("{item_label}.category"), clone));

            let mut clone = analysis.clone();
            clone.imports[index].items[item_index].range = shift_points(item.range);
            out.push((format!("{item_label}.range row/column"), clone));
        }
    }

    // --- References -------------------------------------------------------
    for (index, reference) in analysis.references.iter().enumerate() {
        let label = format!("references[{index}]");

        let mut clone = analysis.clone();
        clone.references[index].kind = different(reference.kind, &REFERENCE_KINDS);
        out.push((format!("{label}.kind"), clone));

        let mut clone = analysis.clone();
        clone.references[index].written = different_string(&reference.written);
        out.push((format!("{label}.written"), clone));

        let mut clone = analysis.clone();
        clone.references[index].range = shift_points(reference.range);
        out.push((format!("{label}.range row/column"), clone));

        let mut clone = analysis.clone();
        clone.references[index].scope_id = foreign_id();
        out.push((format!("{label}.scope_id"), clone));

        let mut clone = analysis.clone();
        clone.references[index].declaration_id = Some(foreign_id());
        out.push((format!("{label}.declaration_id"), clone));
    }

    // --- Call-like occurrences -------------------------------------------
    for (index, call) in analysis.calls.iter().enumerate() {
        let label = format!("calls[{index}]");

        let mut clone = analysis.clone();
        clone.calls[index].form = different(call.form, &CALL_FORMS);
        out.push((format!("{label}.form"), clone));

        let mut clone = analysis.clone();
        clone.calls[index].callee_written = different_string(&call.callee_written);
        out.push((format!("{label}.callee_written"), clone));

        let mut clone = analysis.clone();
        clone.calls[index].scope_id = foreign_id();
        out.push((format!("{label}.scope_id"), clone));

        let mut clone = analysis.clone();
        clone.calls[index].expression_range = shift_points(call.expression_range);
        out.push((format!("{label}.expression_range row/column"), clone));

        let mut clone = analysis.clone();
        clone.calls[index].callee_range = shift_points(call.callee_range);
        out.push((format!("{label}.callee_range row/column"), clone));

        let mut clone = analysis.clone();
        clone.calls[index].callee_range = shift_columns(call.callee_range);
        out.push((format!("{label}.callee_range column"), clone));

        let mut clone = analysis.clone();
        clone.calls[index].type_arguments = Some(match call.type_arguments.as_deref() {
            Some(arguments) => different_string(arguments),
            None => "[mutated]".to_string(),
        });
        out.push((format!("{label}.type_arguments"), clone));

        // The type-argument range: missing from the previous rendering.
        let mut clone = analysis.clone();
        clone.calls[index].type_arguments_range = Some(different_range(call.type_arguments_range));
        out.push((format!("{label}.type_arguments_range"), clone));

        let mut clone = analysis.clone();
        clone.calls[index].dynamic_callee = !call.dynamic_callee;
        out.push((format!("{label}.dynamic_callee"), clone));

        let mut clone = analysis.clone();
        clone.calls[index].nullsafe = !call.nullsafe;
        out.push((format!("{label}.nullsafe"), clone));
    }

    // --- File-level test evidence ----------------------------------------
    for (index, evidence) in analysis.file_test_evidence.iter().enumerate() {
        let label = format!("file_test_evidence[{index}]");

        let mut clone = analysis.clone();
        clone.file_test_evidence[index].kind = different(evidence.kind, &TEST_EVIDENCE_KINDS);
        out.push((format!("{label}.kind"), clone));

        let mut clone = analysis.clone();
        clone.file_test_evidence[index].detail = different_string(&evidence.detail);
        out.push((format!("{label}.detail"), clone));

        let mut clone = analysis.clone();
        clone.file_test_evidence[index].range = shift_points(evidence.range);
        out.push((format!("{label}.range row/column"), clone));

        let mut clone = analysis.clone();
        clone.file_test_evidence[index].range = shift_columns(evidence.range);
        out.push((format!("{label}.range column"), clone));
    }

    // --- Recovery regions -------------------------------------------------
    for (index, region) in analysis.recovery_regions.iter().enumerate() {
        let mut clone = analysis.clone();
        clone.recovery_regions[index] = shift_points(*region);
        out.push((format!("recovery_regions[{index}] row/column"), clone));

        let mut clone = analysis.clone();
        clone.recovery_regions[index] = shift_columns(*region);
        out.push((format!("recovery_regions[{index}] column"), clone));
    }

    // --- Receiver-type evidence -------------------------------------------
    for (index, evidence) in analysis.receiver_type_evidence.iter().enumerate() {
        let label = format!("receiver_type_evidence[{index}]");

        let mut clone = analysis.clone();
        clone.receiver_type_evidence[index].kind =
            different(evidence.kind, &RECEIVER_EVIDENCE_KINDS);
        out.push((format!("{label}.kind"), clone));

        let mut clone = analysis.clone();
        clone.receiver_type_evidence[index].receiver = different_string(&evidence.receiver);
        out.push((format!("{label}.receiver"), clone));

        let mut clone = analysis.clone();
        clone.receiver_type_evidence[index].scope_id = foreign_id();
        out.push((format!("{label}.scope_id"), clone));

        let mut clone = analysis.clone();
        clone.receiver_type_evidence[index].receiver_range = shift_points(evidence.receiver_range);
        out.push((format!("{label}.receiver_range row/column"), clone));

        let mut clone = analysis.clone();
        clone.receiver_type_evidence[index].written = different_string(&evidence.written);
        out.push((format!("{label}.written"), clone));

        let mut clone = analysis.clone();
        clone.receiver_type_evidence[index].written_range = shift_columns(evidence.written_range);
        out.push((format!("{label}.written_range column"), clone));

        let mut clone = analysis.clone();
        clone.receiver_type_evidence[index].evidence_range = shift_points(evidence.evidence_range);
        out.push((format!("{label}.evidence_range row/column"), clone));

        let mut clone = analysis.clone();
        clone.receiver_type_evidence[index].language = different(evidence.language, &LANGUAGES);
        out.push((format!("{label}.language"), clone));

        let mut clone = analysis.clone();
        clone.receiver_type_evidence[index].relative_path =
            different_string(&evidence.relative_path);
        out.push((format!("{label}.relative_path"), clone));

        let mut clone = analysis.clone();
        clone.receiver_type_evidence[index].snapshot_id = different_string(&evidence.snapshot_id);
        out.push((format!("{label}.snapshot_id"), clone));
    }

    // --- Diagnostics ------------------------------------------------------
    for (index, diagnostic) in analysis.diagnostics.iter().enumerate() {
        let label = format!("diagnostics[{index}]");

        let mut clone = analysis.clone();
        clone.diagnostics[index].severity = different(diagnostic.severity, &DIAGNOSTIC_SEVERITIES);
        out.push((format!("{label}.severity"), clone));

        let mut clone = analysis.clone();
        clone.diagnostics[index].kind = different(diagnostic.kind, &DIAGNOSTIC_KINDS);
        out.push((format!("{label}.kind"), clone));

        let mut clone = analysis.clone();
        clone.diagnostics[index].message = different_string(&diagnostic.message);
        out.push((format!("{label}.message"), clone));

        let mut clone = analysis.clone();
        clone.diagnostics[index].range = Some(different_range(diagnostic.range));
        out.push((format!("{label}.range"), clone));
    }

    out
}

/// The canonical text and structural equality must agree, field by field.
#[test]
fn every_normalized_field_is_visible_to_both_comparisons() {
    let mut checked = 0usize;
    let mut failures = Vec::new();
    let mut kinds_seen = std::collections::BTreeSet::new();

    for relative in support::all_fixture_files() {
        let analysis = support::analyze_fixture(&relative);
        kinds_seen.insert(format!(
            "{} scopes={} decls={} imports={} refs={} calls={} file_tests={} recovery={} diags={} rxev={}",
            relative,
            analysis.scopes.len(),
            analysis.declarations.len(),
            analysis.imports.len(),
            analysis.references.len(),
            analysis.calls.len(),
            analysis.file_test_evidence.len(),
            analysis.recovery_regions.len(),
            analysis.diagnostics.len(),
            analysis.receiver_type_evidence.len(),
        ));

        let original_text = analysis.canonical_text();
        for (label, mutated) in mutations(&analysis) {
            checked += 1;
            // Structural equality must notice.
            if mutated.same_facts(&analysis) {
                failures.push(format!("{relative}: {label} is invisible to same_facts"));
                continue;
            }
            // The canonical text must notice too, because the digests and the
            // incremental harness are derived from it.
            if mutated.canonical_text() == original_text {
                failures.push(format!(
                    "{relative}: {label} is invisible to canonical_text"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {checked} field mutations were invisible:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Guard against the test silently degenerating into a no-op.
    assert!(
        checked > 500,
        "expected to check every field of every fact, only checked {checked}"
    );
}

/// The mutations must actually cover every fact kind that the model can carry.
#[test]
fn the_completeness_sweep_covers_every_fact_kind() {
    let mut has_import_item_range = false;
    let mut has_type_arguments_range = false;
    let mut has_declaration_test_evidence = false;
    let mut has_file_test_evidence = false;
    let mut has_recovery_region = false;
    let mut has_diagnostic_range = false;
    let mut has_receiver_type_evidence = false;
    let mut has_optional_alias_range = false;
    let mut has_optional_module_range = false;

    for relative in support::all_fixture_files() {
        let analysis = support::analyze_fixture(&relative);
        has_import_item_range |= analysis.imports.iter().any(|import| {
            import
                .items
                .iter()
                .any(|item| item.target_range.byte_len() > 0)
        });
        has_optional_alias_range |= analysis
            .imports
            .iter()
            .any(|import| import.items.iter().any(|item| item.alias_range.is_some()));
        has_optional_module_range |= analysis
            .imports
            .iter()
            .any(|import| import.module_range.is_some());
        has_type_arguments_range |= analysis
            .calls
            .iter()
            .any(|call| call.type_arguments_range.is_some());
        has_declaration_test_evidence |= analysis
            .declarations
            .iter()
            .any(|declaration| !declaration.test_evidence.is_empty());
        has_file_test_evidence |= !analysis.file_test_evidence.is_empty();
        has_recovery_region |= !analysis.recovery_regions.is_empty();
        has_diagnostic_range |= analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.range.is_some());
        has_receiver_type_evidence |= !analysis.receiver_type_evidence.is_empty();
    }

    for (present, what) in [
        (has_import_item_range, "import item target ranges"),
        (has_optional_alias_range, "import item alias ranges"),
        (has_optional_module_range, "import module ranges"),
        (has_type_arguments_range, "call type-argument ranges"),
        (has_declaration_test_evidence, "declaration test evidence"),
        (has_file_test_evidence, "file-level test evidence"),
        (has_recovery_region, "recovery regions"),
        (has_diagnostic_range, "diagnostic ranges"),
        (has_receiver_type_evidence, "receiver-type evidence"),
    ] {
        assert!(
            present,
            "the fixture corpus must exercise {what} for this sweep to be meaningful"
        );
    }
}

/// A nested field that only exists on one shape must still be compared.
///
/// `TestEvidence` and `ImportItem` are the two nested fact types, so they get an
/// explicit structural check in addition to the sweep.
#[test]
fn nested_fact_types_compare_their_ranges() {
    let analysis = support::analyze_fixture("go/sample_test.go");
    let evidence: &TestEvidence = analysis
        .declarations
        .iter()
        .flat_map(|declaration| declaration.test_evidence.iter())
        .next()
        .expect("a test evidence entry");
    assert!(evidence.range.byte_len() > 0);

    let mut mutated = analysis.clone();
    mutated.declarations[0].test_evidence = vec![TestEvidence {
        range: shift_points(evidence.range),
        ..evidence.clone()
    }];
    assert!(!mutated.same_facts(&analysis));
    assert_ne!(mutated.canonical_text(), analysis.canonical_text());

    let php = support::analyze_fixture("php/declarations.php");
    let item = php
        .imports
        .iter()
        .flat_map(|import| import.items.iter())
        .next()
        .expect("an import item");
    let mut mutated = php.clone();
    mutated.imports[0].items[0].range = shift_points(item.range);
    assert!(!mutated.same_facts(&php));
    assert_ne!(mutated.canonical_text(), php.canonical_text());
}
