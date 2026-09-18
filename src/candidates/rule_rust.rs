//! `rust.call.local_function_candidate`: bounded lexical/module-local candidate
//! search for Rust plain-name calls.
//!
//! The rule is deliberately narrow and **file-local**. For every Rust call-like
//! occurrence whose form is `plain_name` and whose callee is exactly one
//! written identifier, it searches eligible `function` declarations in the
//! innermost enclosing module:
//!
//! ```text
//! start at the call's containing scope
//!   -> collect eligible `function` declarations at that lexical level
//!   -> first level with a match wins
//!   -> never cross a `module` or `file` boundary
//! ```
//!
//! Everything else is `OutOfScope`. Nothing here resolves a call: one candidate
//! is still only a candidate.

use crate::model::{CallLikeForm, DeclarationKind, FileAnalysis, LanguageId, Scope, ScopeKind};

use crate::links::model::{FactKind, FactLocator};

use super::model::{
    candidate_rule, CallCandidateRecord, CandidateOutcome, CandidateProvenance, CandidateTarget,
};

/// Every Rust call-candidate record derivable from the snapshot.
///
/// Emits one record per Rust call-like occurrence: a candidate record for each
/// in-scope plain-name call, and an `OutOfScope` record for every other call
/// shape, so the artifact is a complete disposition of the calls the snapshot
/// contains.
pub fn candidates(analyses: &[FileAnalysis]) -> Vec<CallCandidateRecord> {
    let mut records = Vec::new();
    for analysis in analyses {
        if analysis.file.language != LanguageId::Rust {
            continue;
        }
        let path = analysis.file.relative_path.as_str();
        for call in &analysis.calls {
            let source = FactLocator {
                relative_path: path.to_string(),
                fact_kind: FactKind::Call,
                fact_id: call.call_id,
                item_index: None,
            };
            let scope_path = analysis.scope_path(call.scope_id);
            let provenance =
                |evidence: Vec<String>,
                 search_levels: Option<u32>,
                 enclosing_module: Option<String>| CandidateProvenance {
                    scope_path: scope_path.clone(),
                    search_levels,
                    enclosing_module,
                    evidence,
                };

            let (outcome, provenance) = match in_scope_reason(call) {
                Some(reason) => (
                    CandidateOutcome::out_of_scope(reason),
                    provenance(Vec::new(), None, None),
                ),
                None => {
                    let (outcome, search_levels, enclosing_module, evidence) =
                        lexical_candidates(analysis, call);
                    (
                        outcome,
                        provenance(evidence, Some(search_levels), Some(enclosing_module)),
                    )
                }
            };
            records.push(CallCandidateRecord::new(
                "rust",
                candidate_rule::RUST_CALL_LOCAL_FUNCTION_CANDIDATE,
                source,
                call.callee_written.clone(),
                outcome,
                provenance,
            ));
        }
    }
    records
}

/// Why a call is outside the rule, or `None` when it is in scope.
///
/// In scope is exactly: `plain_name` form, a non-dynamic callee, and a callee
/// that is exactly one written identifier. Everything else names its reason.
fn in_scope_reason(call: &crate::model::CallLikeOccurrence) -> Option<String> {
    if call.form != CallLikeForm::PlainName {
        return Some(format!(
            "call form `{}` is not `plain_name`",
            call.form.as_str()
        ));
    }
    if call.dynamic_callee {
        return Some("the callee is dynamic, not a written identifier".to_string());
    }
    if as_identifier(&call.callee_written).is_none() {
        return Some(format!(
            "callee `{}` is not a single written identifier",
            call.callee_written
        ));
    }
    None
}

/// The lexical/module-local candidate search for one in-scope call.
///
/// Returns the outcome plus the number of levels ascended, the enclosing module
/// scope that bounded the search, and the evidence.
fn lexical_candidates(
    analysis: &FileAnalysis,
    call: &crate::model::CallLikeOccurrence,
) -> (CandidateOutcome, u32, String, Vec<String>) {
    let path = analysis.file.relative_path.as_str();
    let name = call.callee_written.as_str();
    let chain = lexical_chain(analysis, call.scope_id);
    let enclosing_module = chain
        .last()
        .copied()
        .map(render_scope)
        .unwrap_or_else(|| "<none>".to_string());

    for (level_index, scope) in chain.iter().enumerate() {
        let level = scope.scope_id;
        let scope_name = render_scope(scope);
        let matches: Vec<CandidateTarget> = analysis
            .declarations
            .iter()
            .filter(|declaration| {
                declaration.scope_id == level
                    && declaration.kind == DeclarationKind::Function
                    && declaration.name == name
            })
            .map(|declaration| {
                CandidateTarget::new(
                    path,
                    declaration.declaration_id,
                    declaration.kind.as_str(),
                    declaration.name.clone(),
                    scope_name.clone(),
                )
            })
            .collect();
        if !matches.is_empty() {
            let evidence = vec![
                format!("matched_level={level_index}"),
                format!("scope={scope_name}"),
                format!("enclosing_module={enclosing_module}"),
            ];
            return (
                CandidateOutcome::from_candidates(
                    matches,
                    "unreachable: matches were non-empty".to_string(),
                ),
                level_index as u32,
                enclosing_module,
                evidence,
            );
        }
    }

    let evidence = vec![
        format!("searched_levels={}", chain.len().saturating_sub(1)),
        format!("enclosing_module={enclosing_module}"),
        "candidate_declarations=function".to_string(),
    ];
    (
        CandidateOutcome::no_candidate(format!(
            "no `function` declaration named `{name}` in `{enclosing_module}`"
        )),
        chain.len().saturating_sub(1) as u32,
        enclosing_module,
        evidence,
    )
}

/// The lexical chain from the call's own scope up to and including the
/// innermost enclosing module, innermost first.
///
/// The chain always stops at the first `module` or `file` scope, so the search
/// is confined to the innermost enclosing module and can never borrow a
/// declaration from a parent module or a sibling module.
fn lexical_chain(analysis: &FileAnalysis, start: u32) -> Vec<&Scope> {
    let mut chain = Vec::new();
    let mut level = start;
    let mut guard = 0usize;
    while let Some(scope) = analysis.scopes.get(level as usize) {
        chain.push(scope);
        if is_module_boundary(scope.kind) {
            break;
        }
        match scope.parent_scope_id {
            Some(parent) => level = parent,
            None => break,
        }
        guard += 1;
        if guard > analysis.scopes.len() {
            break;
        }
    }
    chain
}

/// Whether a scope kind bounds a Rust module.
///
/// `file` and `module` are the only boundaries: every other Rust scope kind
/// (function, closure, method, impl, trait, struct, enum, extern block) is
/// transparent for the bounded lexical search, because items declared at module
/// level are visible inside all of them.
fn is_module_boundary(kind: ScopeKind) -> bool {
    matches!(kind, ScopeKind::File | ScopeKind::Module)
}

/// Render a scope for provenance, e.g. `module:a`, `function:run`, `file`.
fn render_scope(scope: &Scope) -> String {
    match &scope.name {
        Some(name) => format!("{}:{name}", scope.kind.as_str()),
        None => scope.kind.as_str().to_string(),
    }
}

/// Whether a written callee is exactly one Rust identifier.
///
/// Accepts an optional `r#` raw-identifier prefix. This is a syntactic check
/// only; it does not try to be a lexer.
fn as_identifier(written: &str) -> Option<&str> {
    let name = written.strip_prefix("r#").unwrap_or(written);
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_alphabetic() || first == '_' => {}
        _ => return None,
    }
    if chars.all(|c| c.is_alphanumeric() || c == '_') {
        Some(name)
    } else {
        None
    }
}
