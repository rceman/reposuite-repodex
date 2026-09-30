//! Bounded PHP call-candidate rules.
//!
//! Conservative V1 layer (PHP_CALL_CANDIDATES_V1): turns existing PHP
//! namespace/import/lexical-class syntax evidence into bounded `call_candidate`
//! evidence. No runtime dispatch, no autoload, no container, no type inference.
//!
//! PHP name semantics applied here (case-sensitivity per PHP):
//!   class-like / function / method names are CASE-INSENSITIVE — lookup keys
//!   are lowercased. Unqualified FUNCTION calls fall back `ns\name` -> global;
//!   qualified and class names never fall back to global.
//!
//! Rules (all bounded, provenance-carrying; a candidate is never a resolved
//! call):
//!
//! ```text
//! php.call.namespace_function_candidate     FQN / qualified / same-namespace /
//!                                           global-fallback free functions
//! php.call.imported_function_candidate      `use function` (+ alias)
//! php.call.construction_class_candidate     new Foo / new self (class decl)
//! php.call.static_method_candidate          Foo::m / alias / FQN / namespace\
//! php.call.lexical_self_method_candidate    self::m -> lexical class direct
//! php.call.lexical_this_method_candidate    $this->m -> lexical class direct
//! ```
//!
//! OutOfScope by design: `static::`, `new static`, `parent::`, `$obj->m()`
//! (receiver type unavailable), dynamic member names, `$callable()`,
//! first-class callables (recorded as references, never calls).

use std::collections::HashMap;

use crate::links::model::{FactKind, FactLocator};
use crate::model::{
    CallLikeForm, DeclarationKind, FileAnalysis, ImportCategory, LanguageId, ScopeKind,
};

use super::model::{
    candidate_rule, CallCandidateRecord, CandidateOutcome, CandidateProvenance, CandidateTarget,
};

/// Every PHP call-candidate record derivable from the snapshot — one record
/// per PHP call-like occurrence, the complete disposition of every PHP call.
pub fn candidates(
    analyses: &[FileAnalysis],
    _links: &[crate::links::model::LinkRecord],
    _entities: &[crate::links::model::StructuralEntity],
) -> Vec<CallCandidateRecord> {
    // ---- repository-wide bounded indexes (§39) ----
    // fqn (lowercase `ns\name` or `name`) -> declarations.
    let mut functions: HashMap<String, Vec<(&FileAnalysis, &crate::model::Declaration)>> =
        HashMap::new();
    let mut classes: HashMap<String, Vec<(&FileAnalysis, &crate::model::Declaration)>> =
        HashMap::new();
    for a in analyses {
        if a.file.language != LanguageId::Php {
            continue;
        }
        for d in &a.declarations {
            let ns = enclosing_namespace(a, d.scope_id).unwrap_or_default();
            let fqn = if ns.is_empty() {
                d.name.clone()
            } else {
                format!("{ns}\\{}", d.name)
            };
            let key = fqn.to_lowercase();
            match d.kind {
                DeclarationKind::Function => {
                    functions.entry(key).or_default().push((a, d));
                }
                DeclarationKind::Class
                | DeclarationKind::Interface
                | DeclarationKind::Trait
                | DeclarationKind::Enum => {
                    classes.entry(key).or_default().push((a, d));
                }
                _ => {}
            }
        }
    }

    let mut records = Vec::new();
    for a in analyses {
        if a.file.language != LanguageId::Php {
            continue;
        }
        let path = a.file.relative_path.as_str();
        // per-file maps: class body scope -> its decl; class body scope ->
        // directly declared methods; alias tables from import items.
        let mut class_of_scope: HashMap<u32, &crate::model::Declaration> = HashMap::new();
        for scope in &a.scopes {
            if !matches!(
                scope.kind,
                ScopeKind::Class | ScopeKind::Interface | ScopeKind::Trait | ScopeKind::Enum
            ) {
                continue;
            }
            let Some(name) = &scope.name else { continue };
            if let Some(decl) = a.declarations.iter().find(|d| {
                matches!(
                    d.kind,
                    DeclarationKind::Class
                        | DeclarationKind::Interface
                        | DeclarationKind::Trait
                        | DeclarationKind::Enum
                ) && d.name.eq_ignore_ascii_case(name)
                    && Some(d.scope_id) == scope.parent_scope_id
            }) {
                class_of_scope.insert(scope.scope_id, decl);
            }
        }
        // import alias tables (§13/§14): function imports feed the function
        // rules; normal uses feed qualified-prefix/class resolution. local
        // name -> written qualified target (case-insensitive keys).
        let mut function_aliases: HashMap<String, String> = HashMap::new();
        let mut name_aliases: HashMap<String, String> = HashMap::new();
        for import in &a.imports {
            for item in &import.items {
                let written = match &import.module {
                    Some(module) => format!("{module}\\{}", item.target),
                    None => item.target.clone(),
                };
                let local = item
                    .alias
                    .clone()
                    .unwrap_or_else(|| last_segment(&written).to_string());
                match item.category {
                    ImportCategory::Function => {
                        function_aliases.insert(local.to_lowercase(), written);
                    }
                    ImportCategory::Normal => {
                        name_aliases.insert(local.to_lowercase(), written);
                    }
                    _ => {}
                }
            }
        }
        let lex = Lexical { a, class_of_scope };

        for call in &a.calls {
            let source = FactLocator {
                relative_path: path.to_string(),
                fact_kind: FactKind::Call,
                fact_id: call.call_id,
                item_index: None,
            };
            let scope_path = a.scope_path(call.scope_id);
            let provenance = |evidence: Vec<String>| CandidateProvenance {
                scope_path: scope_path.clone(),
                search_levels: None,
                enclosing_module: lex.namespace_of(call.scope_id),
                evidence,
            };
            let (outcome, prov, rule_id) = match call.form {
                CallLikeForm::PlainName => {
                    plain_function(call, &lex, &function_aliases, &functions, &provenance)
                }
                CallLikeForm::QualifiedPath => {
                    let (o, p) =
                        qualified_function(call, &lex, &name_aliases, &functions, &provenance);
                    (o, p, candidate_rule::PHP_CALL_NAMESPACE_FUNCTION_CANDIDATE)
                }
                CallLikeForm::MemberSelector => {
                    let (o, p) = this_method(call, &lex, &provenance);
                    (o, p, candidate_rule::PHP_CALL_LEXICAL_THIS_METHOD_CANDIDATE)
                }
                CallLikeForm::StaticScoped => {
                    scoped(call, &lex, &name_aliases, &classes, &provenance)
                }
                CallLikeForm::ExplicitConstruction => {
                    let (o, p) = construction(call, &lex, &name_aliases, &classes, &provenance);
                    (o, p, candidate_rule::PHP_CALL_CONSTRUCTION_CLASS_CANDIDATE)
                }
                CallLikeForm::Indirect => (
                    CandidateOutcome::out_of_scope("indirect_or_variable_callable"),
                    provenance(Vec::new()),
                    candidate_rule::PHP_CALL_NAMESPACE_FUNCTION_CANDIDATE,
                ),
                other => (
                    CandidateOutcome::out_of_scope(format!("form={}", other.as_str())),
                    provenance(Vec::new()),
                    candidate_rule::PHP_CALL_NAMESPACE_FUNCTION_CANDIDATE,
                ),
            };
            records.push(CallCandidateRecord::new(
                "php",
                rule_id,
                source,
                call.callee_written.clone(),
                outcome,
                prov,
            ));
        }
    }
    records
}

/// Last `\`-separated segment.
fn last_segment(qualified: &str) -> &str {
    qualified.rsplit('\\').next().unwrap_or(qualified)
}

/// Per-file lexical context.
struct Lexical<'a> {
    a: &'a FileAnalysis,
    /// class body scope_id -> the class-like declaration that owns it.
    class_of_scope: HashMap<u32, &'a crate::model::Declaration>,
}

impl Lexical<'_> {
    /// Written namespace enclosing `scope_id` (may be multi-segment `A\B`).
    fn namespace_of(&self, scope_id: u32) -> Option<String> {
        let mut parts = Vec::new();
        for id in self.a.scope_chain(scope_id) {
            let s = &self.a.scopes[id as usize];
            if s.kind == ScopeKind::Namespace {
                if let Some(n) = &s.name {
                    parts.push(n.clone());
                }
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\\"))
        }
    }

    /// The nearest enclosing class-like declaration for `$this`/`self`.
    fn class_decl(&self, scope_id: u32) -> Option<&crate::model::Declaration> {
        for id in self.a.scope_chain(scope_id).into_iter().rev() {
            if let Some(d) = self.class_of_scope.get(&id) {
                return Some(*d);
            }
        }
        None
    }
}

/// `helper()` — PHP function lookup tiers: `use function` alias, then the
/// call's own namespace, then the global fallback (functions only).
fn plain_function(
    call: &crate::model::CallLikeOccurrence,
    lex: &Lexical,
    function_aliases: &HashMap<String, String>,
    functions: &HashMap<String, Vec<(&FileAnalysis, &crate::model::Declaration)>>,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance, &'static str) {
    if call.dynamic_callee {
        return (
            CandidateOutcome::out_of_scope("variable_callable"),
            provenance(Vec::new()),
            candidate_rule::PHP_CALL_NAMESPACE_FUNCTION_CANDIDATE,
        );
    }
    let name = call.callee_written.as_str();
    // tier 1: `use function` alias.
    if let Some(target) = function_aliases.get(&name.to_lowercase()) {
        let decls: Vec<CandidateTarget> = lookup(functions, target)
            .iter()
            .map(|(a, d)| self::target(a, d))
            .collect();
        return (
            CandidateOutcome::from_candidates(decls, "imported_function_not_in_index"),
            provenance(vec![
                "tier=use_function".to_string(),
                format!("alias={name}"),
                format!("target={target}"),
            ]),
            candidate_rule::PHP_CALL_IMPORTED_FUNCTION_CANDIDATE,
        );
    }
    let ns = lex.namespace_of(call.scope_id);
    // tier 2: same-namespace function.
    if let Some(ns) = &ns {
        if !ns.is_empty() {
            let ns_key = format!("{ns}\\{name}").to_lowercase();
            if let Some(v) = functions.get(&ns_key) {
                if !v.is_empty() {
                    let cands = v.iter().map(|(a, d)| target(a, d)).collect();
                    return (
                        CandidateOutcome::from_candidates(cands, "no_namespace_function"),
                        provenance(vec![
                            "tier=current_namespace".to_string(),
                            format!("namespace={ns}"),
                        ]),
                        candidate_rule::PHP_CALL_NAMESPACE_FUNCTION_CANDIDATE,
                    );
                }
            }
        }
    }
    // tier 3: global fallback (PHP: functions only, never classes).
    let decls: Vec<CandidateTarget> = lookup(functions, name)
        .iter()
        .map(|(a, d)| target(a, d))
        .collect();
    (
        CandidateOutcome::from_candidates(decls, "no_global_function"),
        provenance(vec!["tier=global_fallback".to_string()]),
        candidate_rule::PHP_CALL_NAMESPACE_FUNCTION_CANDIDATE,
    )
}

/// `\A\B\f()` / `A\B\f()` / `namespace\f()` — qualified function calls. The
/// first segment of a non-leading-`\` qualified name may substitute through a
/// class/namespace import alias; no global fallback for qualified names.
fn qualified_function(
    call: &crate::model::CallLikeOccurrence,
    lex: &Lexical,
    name_aliases: &HashMap<String, String>,
    functions: &HashMap<String, Vec<(&FileAnalysis, &crate::model::Declaration)>>,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance) {
    if call.dynamic_callee {
        return (
            CandidateOutcome::out_of_scope("dynamic_callee_expression"),
            provenance(Vec::new()),
        );
    }
    let written = call.callee_written.as_str();
    let fqn = if let Some(rest) = written.strip_prefix('\\') {
        rest.to_string()
    } else if let Some(rest) = written.strip_prefix("namespace\\") {
        match lex.namespace_of(call.scope_id) {
            Some(ns) if !ns.is_empty() => format!("{ns}\\{rest}"),
            _ => rest.to_string(),
        }
    } else {
        // first-segment alias substitution, else current-namespace prefix.
        let first = written.split('\\').next().unwrap_or(written);
        match name_aliases.get(&first.to_lowercase()) {
            Some(prefix) => format!("{prefix}{}", &written[first.len()..]),
            None => match lex.namespace_of(call.scope_id) {
                Some(ns) if !ns.is_empty() => format!("{ns}\\{written}"),
                _ => written.to_string(),
            },
        }
    };
    let decls: Vec<CandidateTarget> = lookup(functions, &fqn)
        .iter()
        .map(|(a, d)| target(a, d))
        .collect();
    (
        CandidateOutcome::from_candidates(decls, "no_indexed_function_for_fqn"),
        provenance(vec![format!("resolved_fqn={fqn}")]),
    )
}

/// `$this->m()` — lexical-class direct methods only. `$obj->m()` is
/// OutOfScope (no receiver type). Dynamic member names are OutOfScope.
fn this_method(
    call: &crate::model::CallLikeOccurrence,
    lex: &Lexical,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance) {
    if call.dynamic_callee {
        return (
            CandidateOutcome::out_of_scope("dynamic_member_name"),
            provenance(Vec::new()),
        );
    }
    let w = call.callee_written.as_str();
    // the object is everything before the LAST `->` — `$this->pages->run()`
    // dispatches on the property value, not on `$this`.
    let Some((object, name)) = w.rsplit_once("->") else {
        return (
            CandidateOutcome::out_of_scope("malformed_member_callee"),
            provenance(Vec::new()),
        );
    };
    if object != "$this" {
        return (
            CandidateOutcome::out_of_scope("receiver_type_unavailable"),
            provenance(vec![format!("receiver={object}")]),
        );
    }
    let Some(class) = lex.class_decl(call.scope_id) else {
        return (
            CandidateOutcome::out_of_scope("no_lexical_class_scope"),
            provenance(Vec::new()),
        );
    };
    let cands = direct_methods(lex.a, class, name);
    (
        CandidateOutcome::from_candidates(
            cands,
            "no_direct_method_in_lexical_class(inherited/trait/magic dispatch not modeled in V1)",
        ),
        provenance(vec![format!("class={}", class.name)]),
    )
}

/// `Foo::m()` / `self::m()` / `static::m()` / `parent::m()`. Literal scope AND
/// literal name only; the member must be a directly declared method of each
/// bounded class candidate (no global method-name scan).
fn scoped(
    call: &crate::model::CallLikeOccurrence,
    lex: &Lexical,
    name_aliases: &HashMap<String, String>,
    classes: &HashMap<String, Vec<(&FileAnalysis, &crate::model::Declaration)>>,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance, &'static str) {
    // `self::m()` carries its own durable rule id (§9); named scopes carry the
    // static-method rule.
    let rule = if call
        .callee_written
        .split("::")
        .next()
        .is_some_and(|s| s.eq_ignore_ascii_case("self"))
    {
        candidate_rule::PHP_CALL_LEXICAL_SELF_METHOD_CANDIDATE
    } else {
        candidate_rule::PHP_CALL_STATIC_METHOD_CANDIDATE
    };
    if call.dynamic_callee {
        return (
            CandidateOutcome::out_of_scope("dynamic_scope_or_member"),
            provenance(Vec::new()),
            rule,
        );
    }
    let w = call.callee_written.as_str();
    let Some((scope, name)) = w.rsplit_once("::") else {
        return (
            CandidateOutcome::out_of_scope("malformed_scoped_callee"),
            provenance(Vec::new()),
            rule,
        );
    };
    match scope.to_lowercase().as_str() {
        "static" => {
            return (
                CandidateOutcome::out_of_scope("late_static_binding"),
                provenance(Vec::new()),
                rule,
            )
        }
        "parent" => {
            return (
                CandidateOutcome::out_of_scope("parent_scope_not_modeled_in_v1"),
                provenance(Vec::new()),
                rule,
            )
        }
        _ => {}
    }
    // self:: -> the lexical class; named scope -> class-name resolution.
    let class_decls: Vec<(&FileAnalysis, &crate::model::Declaration)> =
        if scope.eq_ignore_ascii_case("self") {
            match lex.class_decl(call.scope_id) {
                Some(d) => vec![(lex.a, d)],
                None => {
                    return (
                        CandidateOutcome::out_of_scope("no_lexical_class_scope"),
                        provenance(Vec::new()),
                        rule,
                    )
                }
            }
        } else {
            resolve_class(scope, lex, name_aliases, classes, call.scope_id)
        };
    if class_decls.is_empty() {
        return (
            CandidateOutcome::no_candidate("no_indexed_class_for_scope"),
            provenance(vec![format!("scope={scope}")]),
            rule,
        );
    }
    // every bounded class contributes its DIRECTLY declared methods only.
    let mut cands = Vec::new();
    for (a, class) in &class_decls {
        cands.extend(direct_methods(a, class, name));
    }
    (
        CandidateOutcome::from_candidates(cands, "no_direct_method_in_candidate_class"),
        provenance(vec![
            format!("scope={scope}"),
            format!("classes={}", class_decls.len()),
        ]),
        rule,
    )
}

/// `new Foo()` / `new self()` / `new \A\B\Foo()` / `new namespace\Foo()` —
/// the candidate target is the CLASS declaration, never a claim about
/// `__construct` resolution. `new static()`/`new parent()` are OutOfScope.
fn construction(
    call: &crate::model::CallLikeOccurrence,
    lex: &Lexical,
    name_aliases: &HashMap<String, String>,
    classes: &HashMap<String, Vec<(&FileAnalysis, &crate::model::Declaration)>>,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance) {
    if call.dynamic_callee {
        return (
            CandidateOutcome::out_of_scope("dynamic_class_expression"),
            provenance(Vec::new()),
        );
    }
    let w = call.callee_written.as_str();
    match w.to_lowercase().as_str() {
        "static" => {
            return (
                CandidateOutcome::out_of_scope("late_static_binding"),
                provenance(Vec::new()),
            )
        }
        "parent" => {
            return (
                CandidateOutcome::out_of_scope("parent_scope_not_modeled_in_v1"),
                provenance(Vec::new()),
            )
        }
        _ => {}
    }
    let decls = if w.eq_ignore_ascii_case("self") {
        match lex.class_decl(call.scope_id) {
            Some(d) => vec![(lex.a, d)],
            None => {
                return (
                    CandidateOutcome::out_of_scope("no_lexical_class_scope"),
                    provenance(Vec::new()),
                )
            }
        }
    } else {
        resolve_class(w, lex, name_aliases, classes, call.scope_id)
    };
    let cands = decls.iter().map(|(a, d)| target(a, d)).collect();
    (
        CandidateOutcome::from_candidates(cands, "no_indexed_class"),
        provenance(vec![format!("written={w}")]),
    )
}

/// Directly declared methods of `class` named `name` (case-insensitive, §10)
/// in `a` — the file the class is declared in. Parent/trait/magic members are
/// deliberately out of reach here.
fn direct_methods(
    a: &FileAnalysis,
    class: &crate::model::Declaration,
    name: &str,
) -> Vec<CandidateTarget> {
    let Some(body_scope) = a
        .scopes
        .iter()
        .find(|s| {
            s.parent_scope_id == Some(class.scope_id)
                && s.name.as_deref() == Some(class.name.as_str())
                && matches!(
                    s.kind,
                    ScopeKind::Class | ScopeKind::Interface | ScopeKind::Trait | ScopeKind::Enum
                )
        })
        .map(|s| s.scope_id)
    else {
        return Vec::new();
    };
    a.declarations
        .iter()
        .filter(|d| {
            d.kind == DeclarationKind::Method
                && d.scope_id == body_scope
                && d.name.eq_ignore_ascii_case(name)
        })
        .map(|m| target(a, m))
        .collect()
}

/// Class-name resolution per PHP semantics: `\A\B` FQN; `namespace\X`
/// current-namespace-relative; `A\B` first-segment import alias else
/// current-ns prefix; unqualified `X` alias else `ns\X` (NO global fallback
/// for classes).
fn resolve_class<'a>(
    written: &str,
    lex: &Lexical<'a>,
    name_aliases: &HashMap<String, String>,
    classes: &'a HashMap<String, Vec<(&'a FileAnalysis, &'a crate::model::Declaration)>>,
    scope_id: u32,
) -> Vec<(&'a FileAnalysis, &'a crate::model::Declaration)> {
    let fqn = if let Some(rest) = written.strip_prefix('\\') {
        rest.to_string()
    } else if let Some(rest) = written.strip_prefix("namespace\\") {
        match lex.namespace_of(scope_id) {
            Some(ns) if !ns.is_empty() => format!("{ns}\\{rest}"),
            _ => rest.to_string(),
        }
    } else {
        let first = written.split('\\').next().unwrap_or(written);
        let tail = &written[first.len()..];
        match name_aliases.get(&first.to_lowercase()) {
            Some(prefix) => format!("{prefix}{tail}"),
            None => match lex.namespace_of(scope_id) {
                Some(ns) if !ns.is_empty() => format!("{ns}\\{written}"),
                _ => written.to_string(),
            },
        }
    };
    lookup(classes, &fqn)
}

/// Lowercase-key map lookup.
fn lookup<'a>(
    map: &'a HashMap<String, Vec<(&'a FileAnalysis, &'a crate::model::Declaration)>>,
    fqn: &str,
) -> Vec<(&'a FileAnalysis, &'a crate::model::Declaration)> {
    map.get(&fqn.to_lowercase()).cloned().unwrap_or_default()
}

/// A candidate target record pointing at a declaration.
fn target(a: &FileAnalysis, d: &crate::model::Declaration) -> CandidateTarget {
    CandidateTarget::new(
        a.file.relative_path.clone(),
        d.declaration_id,
        d.kind.as_str(),
        d.name.clone(),
        a.scope_path(d.scope_id),
    )
}

/// The written namespace enclosing `scope_id` — shared shape with the link
/// layer's `enclosing_namespace`, kept local to avoid coupling the candidate
/// rule to link internals.
fn enclosing_namespace(a: &FileAnalysis, scope_id: u32) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for id in a.scope_chain(scope_id) {
        let scope = &a.scopes[id as usize];
        if scope.kind == ScopeKind::Namespace {
            if let Some(name) = &scope.name {
                parts.push(name.clone());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\\"))
    }
}
