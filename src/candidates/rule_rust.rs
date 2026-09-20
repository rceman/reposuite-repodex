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

use std::collections::HashMap;

use crate::model::{
    CallLikeForm, DeclarationKind, FileAnalysis, ImportItem, LanguageId, LocalBindingOccurrence,
    Scope, ScopeKind,
};

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
        // Index local bindings by written name once per file, so each call only
        // inspects the handful of bindings that could shadow its callee.
        let mut bindings_by_name: HashMap<&str, Vec<&LocalBindingOccurrence>> = HashMap::new();
        for binding in &analysis.bindings {
            bindings_by_name
                .entry(binding.name.as_str())
                .or_default()
                .push(binding);
        }
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
                        lexical_candidates(analysis, call, &bindings_by_name);
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
///
/// Search precedence (nearest name-bearing construct controls whether lookup
/// may continue outward):
///
/// ```text
/// a covering same-name `LocalBindingOccurrence`
///     -> stop: `shadowed_by_local_binding` (definite) or
///        `blocked_by_ambiguous_local_binding` (conservative)
/// at each lexical level, innermost first:
///     eligible `function` declarations
///         -> candidate set for that level (a `use`/`const` at the *same* level
///            cannot also be a same-name value, so the `function` wins here)
///     a same-name `constant`/`static` declaration
///         -> stop: `blocked_by_local_constant` (a nearer value item)
///     a same-name `use` import leaf/alias
///         -> stop: `blocked_by_import_binding` (a nearer name-bearing item)
/// nothing relevant -> continue outward
/// ```
fn lexical_candidates(
    analysis: &FileAnalysis,
    call: &crate::model::CallLikeOccurrence,
    bindings_by_name: &HashMap<&str, Vec<&LocalBindingOccurrence>>,
) -> (CandidateOutcome, u32, String, Vec<String>) {
    let path = analysis.file.relative_path.as_str();
    let name = call.callee_written.as_str();
    let chain = lexical_chain(analysis, call.scope_id);
    let enclosing_module = chain
        .last()
        .copied()
        .map(render_scope)
        .unwrap_or_else(|| "<none>".to_string());

    // A covering same-name local binding is the innermost name-bearing
    // construct: it shadows every outer function, constant and import. Its
    // `visibility_ranges` bound the byte region and the transparent-scope gate
    // keeps it from reaching into a nested `fn`/item that does not capture it.
    if let Some((outcome, evidence)) = local_binding_block(
        analysis,
        call,
        bindings_by_name.get(name),
        &enclosing_module,
    ) {
        return (outcome, 0, enclosing_module, evidence);
    }

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

        // A same-name local `const`/`static` is a nearer name-bearing item than
        // any *outer* function, so it blocks outward lookup when no `function`
        // matched at this level. `const`/`static` hoist to their enclosing
        // block and reach nested items in that block's subtree, so
        // `scope_id == level` is the bounded gate.
        if let Some(blocker) = analysis.declarations.iter().find(|declaration| {
            declaration.scope_id == level
                && declaration.kind == DeclarationKind::Constant
                && declaration.name == name
        }) {
            let evidence = vec![
                "blocker=local_constant".to_string(),
                format!("blocker_declaration_id={}", blocker.declaration_id),
                format!("blocker_name={}", blocker.name),
                format!("blocker_scope={scope_name}"),
                format!("matched_level={level_index}"),
                format!("enclosing_module={enclosing_module}"),
            ];
            return (
                CandidateOutcome::no_candidate("blocked_by_local_constant"),
                level_index as u32,
                enclosing_module,
                evidence,
            );
        }

        // A same-name `use` import binds the local leaf/alias in this scope, so
        // a `use` nearer than the nearest `function` shadows it. Like `const`,
        // an import is an item reaching nested items in its block's subtree.
        // Wildcard `use`s bind no single name, so they never block here.
        if let Some((import, item)) = analysis
            .imports
            .iter()
            .filter(|import| import.scope_id == level)
            .find_map(|import| {
                import
                    .items
                    .iter()
                    .find(|item| import_local_name(item) == Some(name))
                    .map(|item| (import, item))
            })
        {
            let mut evidence = vec![
                "blocker=import".to_string(),
                format!("blocker_import_id={}", import.import_id),
                format!("blocker_name={name}"),
                format!("blocker_target={}", item.target),
                format!("blocker_scope={scope_name}"),
                format!("matched_level={level_index}"),
                format!("enclosing_module={enclosing_module}"),
            ];
            if let Some(alias) = &item.alias {
                evidence.push(format!("blocker_alias={alias}"));
            }
            return (
                CandidateOutcome::no_candidate("blocked_by_import_binding"),
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

/// Whether a covering same-name local binding blocks this call.
///
/// A binding applies only when `binding.covers(call_byte)` *and* the binding's
/// scope reaches the call's scope through scopes that capture (`closure`); a
/// `let`/parameter never reaches into a nested `fn`/method/item body, so a
/// `covers` hit that would otherwise over-shadow is filtered out here. Returns
/// the suppressed outcome plus provenance for the *nearest* covering binding
/// (the one introduced latest in source order).
fn local_binding_block(
    analysis: &FileAnalysis,
    call: &crate::model::CallLikeOccurrence,
    same_name: Option<&Vec<&LocalBindingOccurrence>>,
    enclosing_module: &str,
) -> Option<(CandidateOutcome, Vec<String>)> {
    let same_name = same_name?;
    let call_byte = call.callee_range.byte_start;
    let mut covering: Vec<&LocalBindingOccurrence> = same_name
        .iter()
        .filter(|binding| {
            binding.covers(call_byte) && binding_applies(analysis, binding.scope_id, call.scope_id)
        })
        .copied()
        .collect();
    if covering.is_empty() {
        return None;
    }
    // Several bindings can cover one call (sequential `let`s, nested blocks).
    // The one introduced latest is the nearest applicable introduction.
    covering.sort_by_key(|binding| (binding.name_range.byte_start, binding.binding_id));
    let nearest = covering.last().expect("covering is non-empty");

    let visibility = nearest
        .visibility_ranges
        .iter()
        .map(|range| range.render())
        .collect::<Vec<_>>()
        .join(",");
    let reason = if nearest.ambiguous {
        // The fact is binding-like but syntax alone cannot prove it is a fresh
        // local (refutable position may name a constant/variant). Suppress the
        // outer function conservatively rather than manufacture certainty.
        "blocked_by_ambiguous_local_binding"
    } else {
        "shadowed_by_local_binding"
    };
    let scope_path = analysis.scope_path(nearest.scope_id);
    let evidence = vec![
        "blocker=local_binding".to_string(),
        format!("blocker_kind={}", nearest.kind.as_str()),
        format!("blocker_name={}", nearest.name),
        format!("blocker_binding_id={}", nearest.binding_id),
        format!("blocker_ambiguous={}", nearest.ambiguous),
        format!("blocker_scope={scope_path}"),
        format!("blocker_visibility=[{visibility}]"),
        format!("covering_bindings={}", covering.len()),
        format!("enclosing_module={enclosing_module}"),
    ];
    Some((CandidateOutcome::no_candidate(reason), evidence))
}

/// Whether a local binding declared in `binding_scope` is lexically reachable
/// to a call in `call_scope`.
///
/// A `let`/parameter/pattern binding is visible inside its own scope and inside
/// nested closures (which capture it), but **not** inside a nested `fn`, method
/// or item body, which opens a fresh non-capturing scope. So the call's scope
/// may ascend to the binding's scope only through `closure` scopes; the first
/// non-closure scope that is not the binding's own scope ends the search.
fn binding_applies(analysis: &FileAnalysis, binding_scope: u32, call_scope: u32) -> bool {
    let mut level = call_scope;
    let mut guard = 0usize;
    loop {
        if level == binding_scope {
            return true;
        }
        let Some(scope) = analysis.scopes.get(level as usize) else {
            return false;
        };
        if scope.kind != ScopeKind::Closure {
            return false;
        }
        match scope.parent_scope_id {
            Some(parent) => level = parent,
            None => return false,
        }
        guard += 1;
        if guard > analysis.scopes.len() {
            return false;
        }
    }
}

/// The local name a `use` item binds, when it binds a single identifier.
///
/// The bound name is the alias for `use path as name`, otherwise the last `::`
/// segment of the written target. A wildcard (`use a::*`) binds no single name,
/// so it never blocks a call. `use a::b::{self}` binds `b`.
fn import_local_name(item: &ImportItem) -> Option<&str> {
    if item.wildcard {
        return None;
    }
    if let Some(alias) = &item.alias {
        return Some(alias.as_str());
    }
    let mut segments = item.target.rsplit("::");
    match segments.next() {
        Some("self") => segments.next(),
        leaf => leaf,
    }
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
