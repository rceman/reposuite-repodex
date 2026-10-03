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
//! OutOfScope by design: `static::`, `new static`, `$obj->m()`
//! (receiver type unavailable), dynamic member names, `$callable()`,
//! first-class callables (recorded as references, never calls).

use std::collections::{BTreeSet, HashMap};

use crate::links::model::{FactKind, FactLocator, LinkTarget};
use crate::model::{
    CallLikeForm, DeclarationFlag, DeclarationKind, FileAnalysis, LanguageId, ScopeKind,
};

use super::model::{
    candidate_rule, CallCandidateRecord, CandidateOutcome, CandidateProvenance, CandidateTarget,
};

/// Every PHP call-candidate record derivable from the snapshot — one record
/// per PHP call-like occurrence, the complete disposition of every PHP call.
pub fn candidates(
    analyses: &[FileAnalysis],
    links: &[crate::links::model::LinkRecord],
    _entities: &[crate::links::model::StructuralEntity],
) -> Vec<CallCandidateRecord> {
    let hier = Hier::build(analyses, links);
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
            let ns = crate::php_resolve::enclosing_namespace(a, d.scope_id).unwrap_or_default();
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
        // import alias tables from the shared resolver (§7 — one resolver for
        // construction, scoped calls, receiver types and hierarchy links).
        let function_aliases = crate::php_resolve::function_aliases(a);
        let name_aliases = crate::php_resolve::name_aliases(a);
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
                CallLikeForm::MemberSelector => member_call(
                    call,
                    &lex,
                    &Rcx {
                        name_aliases: &name_aliases,
                        classes: &classes,
                        hier: &hier,
                    },
                    &provenance,
                ),
                CallLikeForm::StaticScoped => {
                    scoped(call, &lex, &name_aliases, &classes, &hier, &provenance)
                }
                CallLikeForm::ExplicitConstruction => {
                    construction(call, &lex, &name_aliases, &classes, &hier, &provenance)
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

/// Bounded name-resolution context shared by the PHP rules: import aliases,
/// the FQN class index and the extends-hierarchy index.
struct Rcx<'a> {
    name_aliases: &'a HashMap<String, String>,
    classes: &'a HashMap<String, Vec<(&'a FileAnalysis, &'a crate::model::Declaration)>>,
    hier: &'a Hier<'a>,
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
        crate::php_resolve::enclosing_namespace(self.a, scope_id)
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

    /// The nearest enclosing class-body scope (Class/Interface/Trait/Enum),
    /// regardless of whether a declaration could be matched for it.
    fn class_scope(&self, scope_id: u32) -> Option<u32> {
        for id in self.a.scope_chain(scope_id).into_iter().rev() {
            if matches!(
                self.a.scopes[id as usize].kind,
                ScopeKind::Class | ScopeKind::Interface | ScopeKind::Trait | ScopeKind::Enum
            ) {
                return Some(id);
            }
        }
        None
    }

    /// The nearest enclosing callable scope — where parameters and local
    /// variables live.
    fn callable_scope(&self, scope_id: u32) -> Option<u32> {
        for id in self.a.scope_chain(scope_id).into_iter().rev() {
            if matches!(
                self.a.scopes[id as usize].kind,
                ScopeKind::Function
                    | ScopeKind::Method
                    | ScopeKind::Closure
                    | ScopeKind::ArrowFunction
            ) {
                return Some(id);
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

/// Member calls dispatch on the receiver: `$this->m()` is the lexical-this
/// rule; every other receiver goes through the bounded receiver-type rule,
/// which uses only normalized type-hint / literal-`new` evidence.
fn member_call(
    call: &crate::model::CallLikeOccurrence,
    lex: &Lexical,
    rcx: &Rcx,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance, &'static str) {
    if call.dynamic_callee {
        return (
            CandidateOutcome::out_of_scope("dynamic_member_name"),
            provenance(Vec::new()),
            candidate_rule::PHP_CALL_LEXICAL_THIS_METHOD_CANDIDATE,
        );
    }
    let w = call.callee_written.as_str();
    // the object is everything before the LAST `->` — `$this->pages->run()`
    // dispatches on the property value, not on `$this`. A `?` suffix is the
    // nullsafe marker on the object text (e.g. `$repo?->save`).
    let Some((raw_object, name)) = w.rsplit_once("->") else {
        return (
            CandidateOutcome::out_of_scope("malformed_member_callee"),
            provenance(Vec::new()),
            candidate_rule::PHP_CALL_LEXICAL_THIS_METHOD_CANDIDATE,
        );
    };
    let object = raw_object.trim_end_matches('?');
    if object == "$this" {
        return lexical_this(call, lex, rcx.hier, name, provenance);
    }
    typed_receiver(call, lex, rcx, object, name, provenance)
}

/// `$this->m()` — lexical-class direct method, else the nearest declared
/// ancestor level. A level-0 hit keeps the lexical-this rule id; an
/// ancestor hit carries `php.call.inherited_method_candidate` and the depth
/// in provenance. A class may call its own private methods; ancestor-private
/// methods are never callable.
fn lexical_this(
    call: &crate::model::CallLikeOccurrence,
    lex: &Lexical,
    hier: &Hier,
    name: &str,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance, &'static str) {
    let Some(class) = lex.class_decl(call.scope_id) else {
        return (
            CandidateOutcome::out_of_scope("no_lexical_class_scope"),
            provenance(Vec::new()),
            candidate_rule::PHP_CALL_LEXICAL_THIS_METHOD_CANDIDATE,
        );
    };
    let (depth, cands) = hier.methods_from(&(lex.a, class), name, true);
    let (rule, mut prov) = if depth == 0 {
        (
            candidate_rule::PHP_CALL_LEXICAL_THIS_METHOD_CANDIDATE,
            vec![format!("class={}", class.name)],
        )
    } else {
        (
            candidate_rule::PHP_CALL_INHERITED_METHOD_CANDIDATE,
            vec![
                format!("class={}", class.name),
                format!("ancestor_depth={depth}"),
            ],
        )
    };
    if depth == usize::MAX {
        prov.push("walk=exhausted".to_string());
    }
    (
        CandidateOutcome::from_candidates(
            cands,
            "no_method_on_lexical_class_or_declared_ancestors(trait/magic dispatch not modeled)",
        ),
        provenance(prov),
        rule,
    )
}

/// Bounded receiver-type evidence (RECEIVER_TYPE_EVIDENCE_V1):
///
/// ```text
/// $this->prop  -> property type hints + literal-`new` writes to that property
///                 in the same enclosing class (union; opaque writes do not
///                 remove the may-be-type evidence).
/// $var         -> the callable-scope parameter type hint, UNLESS any write to
///                 $var precedes the call in that scope — then the local-write
///                 rule applies: union of literal-`new` classes written after
///                 the last opaque write; a nearest opaque write or no literal
///                 write at all means the receiver type is unavailable.
/// ```
///
/// Everything else (`$a->b` property chains, `f()->m`, ...) stays
/// `receiver_type_unavailable`. Method lookup is bounded to directly declared
/// methods on the resolved class candidates — never a global name scan.
fn typed_receiver(
    call: &crate::model::CallLikeOccurrence,
    lex: &Lexical,
    rcx: &Rcx,
    object: &str,
    name: &str,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance, &'static str) {
    let call_at = call.expression_range.byte_start;
    let mut evidence: Vec<String> = Vec::new();
    let mut type_texts: Vec<(String, &'static str)> = Vec::new();
    if let Some(prop) = object.strip_prefix("$this->") {
        if prop.is_empty() || !prop.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return (
                CandidateOutcome::out_of_scope("receiver_type_unavailable"),
                provenance(vec![format!("receiver={object}")]),
                candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE,
            );
        }
        let Some(class_scope) = lex.class_scope(call.scope_id) else {
            return (
                CandidateOutcome::out_of_scope("no_lexical_class_scope"),
                provenance(Vec::new()),
                candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE,
            );
        };
        let receiver = object.to_string();
        for ev in &lex.a.receiver_type_evidence {
            if ev.receiver != receiver {
                continue;
            }
            match ev.kind {
                crate::model::ReceiverEvidenceKind::PropertyTypeHint
                    if ev.scope_id == class_scope =>
                {
                    type_texts.push((ev.written.clone(), "property_type_hint"));
                }
                crate::model::ReceiverEvidenceKind::PropertyLiteralNew
                    if lex.class_scope(ev.scope_id) == Some(class_scope) =>
                {
                    type_texts.push((ev.written.clone(), "property_literal_new"));
                }
                _ => {}
            }
        }
        if type_texts.is_empty() {
            return (
                CandidateOutcome::out_of_scope("receiver_type_unavailable"),
                provenance(vec![format!("receiver={object}")]),
                candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE,
            );
        }
    } else if is_simple_variable(object) {
        let Some(callable_scope) = lex.callable_scope(call.scope_id) else {
            return (
                CandidateOutcome::out_of_scope("receiver_type_unavailable"),
                provenance(vec![format!("receiver={object}")]),
                candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE,
            );
        };
        let mut writes: Vec<&crate::model::ReceiverTypeEvidence> = lex
            .a
            .receiver_type_evidence
            .iter()
            .filter(|ev| {
                ev.receiver == object
                    && ev.scope_id == callable_scope
                    && matches!(
                        ev.kind,
                        crate::model::ReceiverEvidenceKind::LocalLiteralNew
                            | crate::model::ReceiverEvidenceKind::LocalOpaqueWrite
                    )
                    && ev.evidence_range.byte_start < call_at
            })
            .collect();
        writes.sort_by_key(|ev| ev.evidence_range.byte_start);
        if writes.is_empty() {
            // §11 rule A: the parameter hint applies only while no assignment
            // to the receiver precedes the call in the same scope.
            for ev in &lex.a.receiver_type_evidence {
                if ev.receiver == object
                    && ev.scope_id == callable_scope
                    && ev.kind == crate::model::ReceiverEvidenceKind::ParameterTypeHint
                {
                    type_texts.push((ev.written.clone(), "parameter_type_hint"));
                }
            }
            if type_texts.is_empty() {
                return (
                    CandidateOutcome::out_of_scope("receiver_type_unavailable"),
                    provenance(vec![format!("receiver={object}")]),
                    candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE,
                );
            }
        } else {
            // Last-write boundary: an opaque nearest write makes the type
            // unknown; otherwise union the literal-`new` classes written
            // after the last opaque write (sequential writes union
            // conservatively — two branch writes are indistinguishable from
            // sequential ones without a CFG, so both stay candidates).
            let last = writes.last().expect("non-empty");
            if last.kind == crate::model::ReceiverEvidenceKind::LocalOpaqueWrite {
                return (
                    CandidateOutcome::out_of_scope("receiver_type_unavailable"),
                    provenance(vec![
                        format!("receiver={object}"),
                        "overwritten_by_opaque_write".to_string(),
                    ]),
                    candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE,
                );
            }
            let last_opaque = writes
                .iter()
                .rposition(|ev| ev.kind == crate::model::ReceiverEvidenceKind::LocalOpaqueWrite)
                .map(|i| writes[i].evidence_range.byte_start);
            for ev in writes {
                if ev.kind == crate::model::ReceiverEvidenceKind::LocalLiteralNew
                    && last_opaque.is_none_or(|at| ev.evidence_range.byte_start > at)
                {
                    type_texts.push((ev.written.clone(), "local_literal_new"));
                }
            }
        }
    } else {
        return (
            CandidateOutcome::out_of_scope("receiver_type_unavailable"),
            provenance(vec![format!("receiver={object}")]),
            candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE,
        );
    }
    // Resolve each written type text into bounded class candidates.
    let mut class_decls: Vec<(&FileAnalysis, &crate::model::Declaration)> = Vec::new();
    let mut seen: std::collections::BTreeSet<(String, u32)> = std::collections::BTreeSet::new();
    for (text, kind) in &type_texts {
        let (resolved, mut notes) = type_text_classes(text, lex, rcx, call.scope_id);
        evidence.push(format!("evidence={kind}"));
        evidence.push(format!("type={text}"));
        evidence.append(&mut notes);
        for (a, d) in resolved {
            if seen.insert((a.file.relative_path.clone(), d.declaration_id)) {
                class_decls.push((a, d));
            }
        }
    }
    if class_decls.is_empty() {
        return (
            CandidateOutcome::no_candidate("no_indexed_class_for_receiver_type"),
            provenance({
                let mut e = vec![format!("receiver={object}")];
                e.extend(evidence);
                e
            }),
            candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE,
        );
    }
    // direct method on each receiver class, else nearest ancestor level. PHP
    // visibility is class-scoped: a receiver typed as the LEXICAL class may
    // reach its private members at level 0; any other class's private members
    // are unreachable at every level.
    let lexical_key = lex.class_decl(call.scope_id).map(|d| Hier::key(lex.a, d));
    let mut cands = Vec::new();
    let mut max_depth = 0usize;
    for (a, class) in &class_decls {
        let same_class = lexical_key.as_ref() == Some(&Hier::key(a, class));
        let (depth, hits) = rcx.hier.methods_from(&(*a, class), name, same_class);
        if depth != usize::MAX {
            max_depth = max_depth.max(depth);
            cands.extend(hits);
        }
    }
    let rule = if max_depth == 0 {
        candidate_rule::PHP_CALL_TYPED_RECEIVER_METHOD_CANDIDATE
    } else {
        candidate_rule::PHP_CALL_INHERITED_METHOD_CANDIDATE
    };
    let mut prov = vec![
        format!("receiver={object}"),
        format!("classes={}", class_decls.len()),
    ];
    prov.extend(evidence);
    if max_depth > 0 {
        prov.push(format!("ancestor_depth={max_depth}"));
    }
    (
        CandidateOutcome::from_candidates(
            cands,
            "no_method_on_typed_receiver_class_or_ancestors(trait/magic dispatch not modeled)",
        ),
        provenance(prov),
        rule,
    )
}

/// `$name` / `$name123` — a bare local variable, nothing else.
fn is_simple_variable(object: &str) -> bool {
    object.starts_with('$')
        && object.len() > 1
        && object[1..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// PHP builtin / pseudo types that never name a repository class.
const PHP_BUILTIN_TYPES: &[&str] = &[
    "int", "float", "string", "bool", "array", "callable", "iterable", "object", "mixed", "void",
    "never", "null", "false", "true",
];

/// Resolve a written type-hint text into bounded class candidates.
///
/// `?T` yields `T`; `A|B` yields the union of both arms (never an arbitrary
/// first pick); `A&B` intersections are unsupported; builtins are filtered;
/// `self` maps to the lexical class, `static`/`parent` stay unsupported.
fn type_text_classes<'a>(
    written: &str,
    lex: &'a Lexical<'a>,
    rcx: &'a Rcx<'a>,
    scope_id: u32,
) -> (
    Vec<(&'a FileAnalysis, &'a crate::model::Declaration)>,
    Vec<String>,
) {
    let mut notes = Vec::new();
    if written.contains('&') {
        notes.push("unsupported=intersection_type".to_string());
        return (Vec::new(), notes);
    }
    let mut out: Vec<(&'a FileAnalysis, &'a crate::model::Declaration)> = Vec::new();
    for arm in written.split('|') {
        let arm = arm.trim().trim_start_matches('?').trim();
        if arm.is_empty() || PHP_BUILTIN_TYPES.contains(&arm.to_lowercase().as_str()) {
            continue;
        }
        if arm.eq_ignore_ascii_case("static") {
            notes.push(format!("unsupported_arm={arm}"));
            continue;
        }
        if arm.eq_ignore_ascii_case("parent") {
            // `parent` binds through the lexical class's declared `extends`
            // link — same machinery as `parent::`. Still bounded evidence.
            if let Some(d) = lex.class_decl(scope_id) {
                let key = Hier::key(lex.a, d);
                let parents = rcx.hier.parent_decls(&key);
                if parents.is_empty() {
                    notes.push("parent_no_indexed_declaration".to_string());
                } else {
                    out.extend(parents);
                }
            } else {
                notes.push("parent_no_lexical_class".to_string());
            }
            continue;
        }
        if arm.eq_ignore_ascii_case("self") {
            if let Some(d) = lex.class_decl(scope_id) {
                out.push((lex.a, d));
            }
            continue;
        }
        out.extend(resolve_class(
            arm,
            lex,
            rcx.name_aliases,
            rcx.classes,
            scope_id,
        ));
    }
    (out, notes)
}

/// `Foo::m()` / `self::m()` / `static::m()` / `parent::m()`. Literal scope AND
/// literal name only; the member must be a directly declared method of each
/// bounded class candidate (no global method-name scan).
fn scoped(
    call: &crate::model::CallLikeOccurrence,
    lex: &Lexical,
    name_aliases: &HashMap<String, String>,
    classes: &HashMap<String, Vec<(&FileAnalysis, &crate::model::Declaration)>>,
    hier: &Hier,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance, &'static str) {
    let scope_head = call
        .callee_written
        .split("::")
        .next()
        .unwrap_or("")
        .to_lowercase();
    let rule = if scope_head == "self" {
        candidate_rule::PHP_CALL_LEXICAL_SELF_METHOD_CANDIDATE
    } else if scope_head == "parent" {
        candidate_rule::PHP_CALL_PARENT_METHOD_CANDIDATE
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
            // `parent::m()` — the lexical class's declared parent
            // candidate(s), then bounded nearest-level lookup starting at
            // that parent. Private members are unreachable at every level.
            let Some(class) = lex.class_decl(call.scope_id) else {
                return (
                    CandidateOutcome::out_of_scope("no_lexical_class_scope"),
                    provenance(Vec::new()),
                    rule,
                );
            };
            let parents = hier.parent_decls(&Hier::key(lex.a, class));
            if parents.is_empty() {
                return (
                    CandidateOutcome::no_candidate("no_indexed_parent_for_lexical_class"),
                    provenance(vec![format!("class={}", class.name)]),
                    rule,
                );
            }
            let mut cands = Vec::new();
            let mut min_depth = usize::MAX;
            for (a, p) in &parents {
                let (depth, hits) = hier.methods_from(&(*a, p), name, false);
                if depth != usize::MAX && depth < min_depth {
                    min_depth = depth;
                }
                cands.extend(hits);
            }
            let mut prov = vec![
                format!("class={}", class.name),
                format!("parents={}", parents.len()),
            ];
            if min_depth != usize::MAX {
                prov.push(format!("ancestor_depth={min_depth}"));
            }
            return (
                CandidateOutcome::from_candidates(
                    cands,
                    "no_method_on_parent_or_ancestors(trait/magic dispatch not modeled)",
                ),
                provenance(prov),
                rule,
            );
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
    // direct method on each bounded class, else nearest ancestor level —
    // `self::` can reach its own private method at level 0; a named external
    // scope cannot. An ancestor hit reports the inherited-method rule.
    let self_scope = scope.eq_ignore_ascii_case("self");
    let mut cands = Vec::new();
    let mut max_depth = 0usize;
    for (a, class) in &class_decls {
        let (depth, hits) = hier.methods_from(&(*a, class), name, self_scope);
        if depth != usize::MAX {
            max_depth = max_depth.max(depth);
            cands.extend(hits);
        }
    }
    let out_rule = if max_depth > 0 {
        candidate_rule::PHP_CALL_INHERITED_METHOD_CANDIDATE
    } else {
        rule
    };
    let mut prov = vec![
        format!("scope={scope}"),
        format!("classes={}", class_decls.len()),
    ];
    if max_depth > 0 {
        prov.push(format!("ancestor_depth={max_depth}"));
    }
    (
        CandidateOutcome::from_candidates(
            cands,
            "no_method_in_candidate_class_or_ancestors(trait/magic dispatch not modeled)",
        ),
        provenance(prov),
        out_rule,
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
    hier: &Hier,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance, &'static str) {
    let rule = candidate_rule::PHP_CALL_CONSTRUCTION_CLASS_CANDIDATE;
    if call.dynamic_callee {
        return (
            CandidateOutcome::out_of_scope("dynamic_class_expression"),
            provenance(Vec::new()),
            rule,
        );
    }
    let w = call.callee_written.as_str();
    match w.to_lowercase().as_str() {
        "static" => {
            return (
                CandidateOutcome::out_of_scope("late_static_binding"),
                provenance(Vec::new()),
                rule,
            )
        }
        "parent" => {
            // `new parent()` — the lexical class's declared parent candidate(s)
            // as construction targets. This is a class candidate only; it does
            // NOT claim `__construct` dispatch resolution.
            let Some(class) = lex.class_decl(call.scope_id) else {
                return (
                    CandidateOutcome::out_of_scope("no_lexical_class_scope"),
                    provenance(Vec::new()),
                    candidate_rule::PHP_CALL_PARENT_CONSTRUCTION_CANDIDATE,
                );
            };
            let parents = hier.parent_decls(&Hier::key(lex.a, class));
            let cands = parents.iter().map(|(a, d)| target(a, d)).collect();
            return (
                CandidateOutcome::from_candidates(cands, "no_indexed_parent_for_lexical_class"),
                provenance(vec![
                    format!("class={}", class.name),
                    format!("parents={}", parents.len()),
                ]),
                candidate_rule::PHP_CALL_PARENT_CONSTRUCTION_CANDIDATE,
            );
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
                    rule,
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
        rule,
    )
}

/// Bounded class-hierarchy index derived from `php.class.extends` link
/// records. `parents` maps a declaring class to ALL declaration candidates of
/// its written parent (Ambiguous preserved). Cycles are guarded by a visited
/// set plus a hard depth bound — valid PHP cannot contain one, malformed
/// source still must not loop.
struct Hier<'a> {
    decls: HashMap<(String, u32), (&'a FileAnalysis, &'a crate::model::Declaration)>,
    parents: HashMap<(String, u32), Vec<(String, u32)>>,
}

/// Defensive traversal bound; real hierarchies are far shorter.
const HIER_DEPTH_BOUND: usize = 64;

impl<'a> Hier<'a> {
    fn build(analyses: &'a [FileAnalysis], links: &[crate::links::model::LinkRecord]) -> Self {
        let mut decls = HashMap::new();
        for a in analyses {
            if a.file.language != LanguageId::Php {
                continue;
            }
            for d in &a.declarations {
                if matches!(
                    d.kind,
                    DeclarationKind::Class
                        | DeclarationKind::Interface
                        | DeclarationKind::Trait
                        | DeclarationKind::Enum
                ) {
                    decls.insert((a.file.relative_path.clone(), d.declaration_id), (a, d));
                }
            }
        }
        let mut parents: HashMap<(String, u32), Vec<(String, u32)>> = HashMap::new();
        for link in links {
            if link.rule_id != crate::links::model::rule::PHP_CLASS_EXTENDS
                || link.source.fact_kind != FactKind::Declaration
            {
                continue;
            }
            let child = (link.source.relative_path.clone(), link.source.fact_id);
            let mut targets = Vec::new();
            for t in link.outcome.all_targets() {
                if let LinkTarget::Declaration {
                    relative_path,
                    declaration_id,
                    ..
                } = t
                {
                    targets.push((relative_path.clone(), *declaration_id));
                }
            }
            parents.entry(child).or_default().extend(targets);
        }
        Self { decls, parents }
    }

    fn key(a: &FileAnalysis, d: &crate::model::Declaration) -> (String, u32) {
        (a.file.relative_path.clone(), d.declaration_id)
    }

    /// Direct parents of `key` that resolve to indexed declarations.
    fn parent_decls(
        &self,
        key: &(String, u32),
    ) -> Vec<(&'a FileAnalysis, &'a crate::model::Declaration)> {
        self.parents
            .get(key)
            .map(|ps| {
                ps.iter()
                    .filter_map(|p| self.decls.get(p).copied())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Nearest-level method lookup from `start`: direct methods first, then
    /// each ancestor level in turn. The FIRST level with any candidate wins
    /// (override precedence) and traversal stops — no all-ancestor dump.
    /// `include_private` holds only at level 0 (a class may call its own
    /// private methods); ancestor-private methods are never callable.
    /// Returns (depth, targets); depth == usize::MAX when nothing matched.
    fn methods_from(
        &self,
        start: &(&'a FileAnalysis, &'a crate::model::Declaration),
        name: &str,
        include_private_at_level0: bool,
    ) -> (usize, Vec<CandidateTarget>) {
        let (a, class) = *start;
        let mut visited: BTreeSet<(String, u32)> = BTreeSet::new();
        visited.insert(Self::key(a, class));
        let first: Vec<_> = methods_named(a, class, name)
            .into_iter()
            .filter(|(_, m)| {
                include_private_at_level0 || !m.flags.contains(&DeclarationFlag::Private)
            })
            .collect();
        if !first.is_empty() {
            return (0, first.into_iter().map(|(fa, m)| target(fa, m)).collect());
        }
        let mut frontier: Vec<(String, u32)> = self
            .parents
            .get(&Self::key(a, class))
            .cloned()
            .unwrap_or_default();
        for depth in 1..=HIER_DEPTH_BOUND {
            let mut hits = Vec::new();
            let mut next = Vec::new();
            for key in &frontier {
                if !visited.insert(key.clone()) {
                    continue;
                }
                let Some((fa, fd)) = self.decls.get(key).copied() else {
                    continue;
                };
                hits.extend(
                    methods_named(fa, fd, name)
                        .into_iter()
                        .filter(|(_, m)| !m.flags.contains(&DeclarationFlag::Private))
                        .map(|(fa2, m)| target(fa2, m)),
                );
                if let Some(ps) = self.parents.get(key) {
                    next.extend(ps.iter().cloned());
                }
            }
            if !hits.is_empty() {
                return (depth, hits);
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        (usize::MAX, Vec::new())
    }
}

/// Directly declared methods of `class` named `name` (case-insensitive) —
/// the raw declaration list used by the hierarchy walk; private methods are
/// NOT filtered here (callers decide per level).
fn methods_named<'a>(
    a: &'a FileAnalysis,
    class: &crate::model::Declaration,
    name: &str,
) -> Vec<(&'a FileAnalysis, &'a crate::model::Declaration)> {
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
        .map(|m| (a, m))
        .collect()
}

/// Class-name resolution: the shared [`crate::php_resolve::class_fqn`]
/// semantics (`\A\B` FQN; `namespace\X` current-namespace; `A\B` alias or
/// current-ns; `X` alias or `ns\X`, NO global fallback) applied to the
/// bounded class index.
fn resolve_class<'a>(
    written: &str,
    lex: &Lexical<'a>,
    name_aliases: &HashMap<String, String>,
    classes: &'a HashMap<String, Vec<(&'a FileAnalysis, &'a crate::model::Declaration)>>,
    scope_id: u32,
) -> Vec<(&'a FileAnalysis, &'a crate::model::Declaration)> {
    let fqn =
        crate::php_resolve::class_fqn(written, lex.namespace_of(scope_id).as_deref(), name_aliases);
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
