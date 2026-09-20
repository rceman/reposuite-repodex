//! Rust language adapter.
//!
//! Extraction is direct AST traversal with named-field lookups, which is the
//! simplest correct strategy for a grammar whose declarations are nested inside
//! `impl`, `trait` and `mod` bodies. The only Tree-sitter query used is the
//! shared recovery query (`queries/rust/recovery.scm`).
//!
//! Boundaries:
//!
//! * macros are never expanded and never produce declarations;
//! * `cfg` attributes are never evaluated;
//! * an `impl Foo` block is a scope, never a declaration of `Foo`;
//! * tuple-struct construction is reported as a call-like occurrence because the
//!   grammar cannot distinguish it from a function call.

use tree_sitter::{Language, Node, Tree};

use crate::model::{
    BindingKind, CallLikeForm, DeclarationFlag, DeclarationKind, ImportCategory, ImportForm,
    LanguageId, ReferenceKind, ScopeKind, SourceRange, TestEvidence, TestEvidenceKind,
};

use super::builder::{import_item, DeclarationDraft, FactBuilder};
use super::recovery::RecoveryScanner;
use super::{AdapterError, GrammarInfo, LanguageAdapter};

pub struct RustAdapter {
    recovery: RecoveryScanner,
}

impl RustAdapter {
    pub fn new() -> Result<Self, AdapterError> {
        let language: Language = tree_sitter_rust::LANGUAGE.into();
        let recovery =
            RecoveryScanner::new(LanguageId::Rust, &language).map_err(|message| AdapterError {
                language: Some(LanguageId::Rust),
                message,
            })?;
        Ok(Self { recovery })
    }
}

const GRAMMAR: GrammarInfo = GrammarInfo {
    crate_name: "tree-sitter-rust",
    crate_version: "0.24.2",
    upstream_repository: "https://github.com/tree-sitter/tree-sitter-rust",
    license: "MIT",
    upstream_queries: &[
        "queries/tags.scm",
        "queries/highlights.scm",
        "queries/injections.scm",
    ],
    repo_queries: &["queries/rust/recovery.scm"],
    root_node_kind: "source_file",
    tree_sitter_compatibility: "ABI 15 (tree-sitter 0.27.x)",
};

impl LanguageAdapter for RustAdapter {
    fn language(&self) -> LanguageId {
        LanguageId::Rust
    }

    fn grammar(&self) -> GrammarInfo {
        GRAMMAR
    }

    fn ts_language(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn recovery(&self) -> &RecoveryScanner {
        &self.recovery
    }

    fn extract(&self, builder: &mut FactBuilder<'_>, tree: &Tree) {
        visit_item_list(builder, tree.root_node());
    }
}

fn visit(builder: &mut FactBuilder<'_>, node: Node) {
    match node.kind() {
        "source_file" | "declaration_list" | "block" => visit_item_list(builder, node),
        "closure_expression" => visit_closure(builder, node),
        "for_expression" => {
            emit_for_bindings(builder, node);
            visit_children(builder, node);
        }
        "match_arm" => {
            emit_match_bindings(builder, node);
            visit_children(builder, node);
        }
        "if_expression" => {
            emit_if_let_bindings(builder, node);
            visit_children(builder, node);
        }
        "while_expression" => {
            emit_while_let_bindings(builder, node);
            visit_children(builder, node);
        }
        "call_expression" => {
            emit_call(builder, node);
            visit_children(builder, node);
        }
        "macro_invocation" => {
            emit_macro_invocation(builder, node);
            visit_children(builder, node);
        }
        _ => visit_children(builder, node),
    }
}

fn visit_children(builder: &mut FactBuilder<'_>, node: Node) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            visit(builder, child);
        }
    }
}

/// Walk a list of items/statements, accumulating attributes so they can be
/// attached to the declaration they annotate.
fn visit_item_list(builder: &mut FactBuilder<'_>, container: Node) {
    let mut cursor = container.walk();
    let mut pending_attributes: Vec<Node> = Vec::new();
    for child in container.children(&mut cursor) {
        if child.kind() == "attribute_item" {
            pending_attributes.push(child);
            continue;
        }
        if !child.is_named() {
            continue;
        }
        // A `let` statement is not a declaration, but it introduces local
        // bindings whose visibility is bounded by this enclosing `container`
        // block. Emit them here so the block range is in hand.
        if child.kind() == "let_declaration" {
            emit_let_bindings(builder, child, container);
        }
        if !visit_declaration(builder, child, &pending_attributes) {
            visit(builder, child);
        }
        pending_attributes.clear();
    }
}

/// Handle a node that declares something. Returns `false` when the node is not
/// a declaration and should be walked as ordinary syntax.
fn visit_declaration(builder: &mut FactBuilder<'_>, node: Node, attributes: &[Node]) -> bool {
    match node.kind() {
        "mod_item" => handle_mod(builder, node),
        "function_item" => handle_function(builder, node, attributes, true),
        "function_signature_item" => handle_function(builder, node, attributes, false),
        "struct_item" => handle_struct_like(builder, node, false),
        "union_item" => handle_struct_like(builder, node, true),
        "foreign_mod_item" => handle_foreign_mod(builder, node),
        "enum_item" => handle_enum(builder, node),
        "trait_item" => handle_trait(builder, node),
        "type_item" => handle_type_alias(builder, node),
        "impl_item" => handle_impl(builder, node),
        "const_item" => handle_const(builder, node),
        "static_item" => handle_static(builder, node),
        "use_declaration" => handle_use(builder, node),
        "macro_definition" => handle_macro_definition(builder, node),
        _ => return false,
    }
    true
}

fn handle_mod(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = builder.text(name_node).to_string();
    let range = builder.range(node);
    let body = node.child_by_field_name("body");
    let body_range = body.map(|body| builder.range(body));
    builder.push_declaration(
        DeclarationDraft::new(
            DeclarationKind::Module,
            name.clone(),
            builder.range(name_node),
            range,
        )
        .with_body(body_range),
    );
    if let Some(body) = body {
        let header = header_range(builder, node, body);
        builder.push_scope(ScopeKind::Module, Some(name), range, header);
        visit_item_list(builder, body);
        builder.pop_scope();
    }
}

fn handle_function(builder: &mut FactBuilder<'_>, node: Node, attributes: &[Node], has_body: bool) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let scope_kind = builder.current_scope_kind();
    let is_method = matches!(scope_kind, ScopeKind::Impl | ScopeKind::Trait);
    let kind = if is_method {
        DeclarationKind::Method
    } else {
        DeclarationKind::Function
    };
    let name = builder.text(name_node).to_string();
    let range = builder.range(node);
    let body = node.child_by_field_name("body");
    let body_range = if has_body {
        body.map(|body| builder.range(body))
    } else {
        None
    };

    let mut draft = DeclarationDraft::new(kind, name.clone(), builder.range(name_node), range)
        .with_body(body_range);
    for modifier in modifier_tokens(node) {
        match modifier {
            "async" => draft = draft.with_flag(DeclarationFlag::Async),
            "unsafe" => draft = draft.with_flag(DeclarationFlag::Unsafe),
            "const" => draft = draft.with_flag(DeclarationFlag::Const),
            _ => {}
        }
    }
    if has_self_parameter(node) {
        draft = draft.with_flag(DeclarationFlag::Receiver);
    }
    if scope_kind == ScopeKind::Trait && has_body {
        draft = draft.with_flag(DeclarationFlag::Default);
    }
    let declaration_id = builder.push_declaration(draft);

    for attribute in attributes {
        if let Some(evidence) = test_evidence_from_attribute(builder, *attribute) {
            builder.add_test_evidence(declaration_id, evidence);
        }
    }

    if let Some(body) = body {
        let scope = if is_method {
            ScopeKind::Method
        } else {
            ScopeKind::Function
        };
        builder.push_scope(scope, Some(name), range, None);
        // Parameters live in the new function scope and cover the body.
        emit_parameter_bindings(builder, node, Some(builder.range(body)));
        visit_item_list(builder, body);
        builder.pop_scope();
    } else {
        // A signature-only item has no body scope; its parameters still bind a
        // name but cover nothing.
        emit_parameter_bindings(builder, node, None);
    }
}

/// Anonymous `async`/`unsafe`/`const` tokens inside `function_modifiers`.
fn modifier_tokens<'tree>(node: Node<'tree>) -> Vec<&'static str> {
    let mut modifiers = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "function_modifiers" {
            continue;
        }
        let mut inner = child.walk();
        for token in child.children(&mut inner) {
            match token.kind() {
                "async" => modifiers.push("async"),
                "unsafe" => modifiers.push("unsafe"),
                "const" => modifiers.push("const"),
                _ => {}
            }
        }
    }
    modifiers
}

fn has_self_parameter(node: Node) -> bool {
    let Some(parameters) = node.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = parameters.walk();
    let found = parameters
        .children(&mut cursor)
        .any(|child| child.kind() == "self_parameter");
    found
}

/// `#[test]` and `<crate>::test` attributes. `#[cfg(test)]`, `#[ignore]` and
/// `#[should_panic]` are deliberately not test evidence on their own.
fn test_evidence_from_attribute(
    builder: &FactBuilder<'_>,
    attribute_item: Node,
) -> Option<TestEvidence> {
    let mut cursor = attribute_item.walk();
    for child in attribute_item.children(&mut cursor) {
        if child.kind() != "attribute" {
            continue;
        }
        let mut inner = child.walk();
        let path = child.children(&mut inner).find(|node| {
            matches!(
                node.kind(),
                "identifier" | "scoped_identifier" | "crate" | "self" | "super" | "metavariable"
            )
        })?;
        let written = builder.text(path);
        let last_segment = written.rsplit("::").next().unwrap_or(written);
        if last_segment == "test" {
            return Some(TestEvidence {
                kind: TestEvidenceKind::Attribute,
                detail: format!("#[{written}]"),
                range: builder.range(attribute_item),
            });
        }
        return None;
    }
    None
}

fn handle_struct_like(builder: &mut FactBuilder<'_>, node: Node, is_union: bool) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    // A Rust `union` is not a struct. Reporting it as one would be a wrong fact.
    let (declaration_kind, scope_kind) = if is_union {
        (DeclarationKind::Union, ScopeKind::Union)
    } else {
        (DeclarationKind::Struct, ScopeKind::Struct)
    };
    let name = builder.text(name_node).to_string();
    let range = builder.range(node);
    let body = node.child_by_field_name("body");
    let body_range = body.map(|body| builder.range(body));
    builder.push_declaration(
        DeclarationDraft::new(
            declaration_kind,
            name.clone(),
            builder.range(name_node),
            range,
        )
        .with_body(body_range),
    );
    let Some(body) = body else {
        return;
    };
    let header = header_range(builder, node, body);
    builder.push_scope(scope_kind, Some(name), range, header);
    if body.kind() == "field_declaration_list" {
        emit_field_declarations(builder, body, DeclarationKind::Field);
    }
    builder.pop_scope();
}

/// Rust `extern "C" { .. }`.
///
/// The block is a scope so that two extern blocks declaring the same foreign
/// name stay distinguishable. The `extern` ABI string is not a fact.
fn handle_foreign_mod(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let range = builder.range(node);
    let header = header_range(builder, node, body);
    builder.push_scope(ScopeKind::ExternBlock, None::<String>, range, header);
    visit_item_list(builder, body);
    builder.pop_scope();
}

fn emit_field_declarations(builder: &mut FactBuilder<'_>, body: Node, kind: DeclarationKind) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "field_declaration" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        builder.push_declaration(DeclarationDraft::new(
            kind,
            builder.text(name_node).to_string(),
            builder.range(name_node),
            builder.range(child),
        ));
    }
}

fn handle_enum(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = builder.text(name_node).to_string();
    let range = builder.range(node);
    let body = node.child_by_field_name("body");
    let body_range = body.map(|body| builder.range(body));
    builder.push_declaration(
        DeclarationDraft::new(
            DeclarationKind::Enum,
            name.clone(),
            builder.range(name_node),
            range,
        )
        .with_body(body_range),
    );
    let Some(body) = body else {
        return;
    };
    let header = header_range(builder, node, body);
    builder.push_scope(ScopeKind::Enum, Some(name), range, header);
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "enum_variant" {
            continue;
        }
        let Some(variant_name) = child.child_by_field_name("name") else {
            continue;
        };
        builder.push_declaration(DeclarationDraft::new(
            DeclarationKind::Variant,
            builder.text(variant_name).to_string(),
            builder.range(variant_name),
            builder.range(child),
        ));
    }
    builder.pop_scope();
}

fn handle_trait(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = builder.text(name_node).to_string();
    let range = builder.range(node);
    let body = node.child_by_field_name("body");
    let body_range = body.map(|body| builder.range(body));
    builder.push_declaration(
        DeclarationDraft::new(
            DeclarationKind::Trait,
            name.clone(),
            builder.range(name_node),
            range,
        )
        .with_body(body_range),
    );
    if let Some(body) = body {
        let header = header_range(builder, node, body);
        builder.push_scope(ScopeKind::Trait, Some(name), range, header);
        visit_item_list(builder, body);
        builder.pop_scope();
    }
}

fn handle_type_alias(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    builder.push_declaration(DeclarationDraft::new(
        DeclarationKind::TypeAlias,
        builder.text(name_node).to_string(),
        builder.range(name_node),
        builder.range(node),
    ));
}

/// `impl Foo` / `impl Trait for Foo` produce a scope plus two unresolved
/// references. They never produce a declaration of the target type.
fn handle_impl(builder: &mut FactBuilder<'_>, node: Node) {
    let type_node = node.child_by_field_name("type");
    let trait_node = node.child_by_field_name("trait");
    let body = node.child_by_field_name("body");
    let range = builder.range(node);
    let header = body.and_then(|body| header_range(builder, node, body));
    builder.push_scope(ScopeKind::Impl, None::<String>, range, header);
    if let Some(trait_node) = trait_node {
        builder.push_reference(
            ReferenceKind::ImplTrait,
            builder.text(trait_node),
            builder.range(trait_node),
            None,
        );
    }
    if let Some(type_node) = type_node {
        builder.push_reference(
            ReferenceKind::ImplTarget,
            builder.text(type_node),
            builder.range(type_node),
            None,
        );
    }
    if let Some(body) = body {
        visit_item_list(builder, body);
    }
    builder.pop_scope();
}

fn handle_const(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    builder.push_declaration(DeclarationDraft::new(
        DeclarationKind::Constant,
        builder.text(name_node).to_string(),
        builder.range(name_node),
        builder.range(node),
    ));
}

fn handle_static(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let mut draft = DeclarationDraft::new(
        DeclarationKind::Constant,
        builder.text(name_node).to_string(),
        builder.range(name_node),
        builder.range(node),
    )
    .with_flag(DeclarationFlag::Static);
    let mut cursor = node.walk();
    if node
        .children(&mut cursor)
        .any(|child| child.kind() == "mutable_specifier")
    {
        draft = draft.with_flag(DeclarationFlag::Mutable);
    }
    builder.push_declaration(draft);
}

fn handle_macro_definition(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    builder.push_declaration(DeclarationDraft::new(
        DeclarationKind::Macro,
        builder.text(name_node).to_string(),
        builder.range(name_node),
        builder.range(node),
    ));
}

fn handle_use(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(argument) = node.child_by_field_name("argument") else {
        return;
    };
    let mut items = Vec::new();
    let mut saw_group = false;
    let mut saw_wildcard = false;
    let mut module: Option<String> = None;
    let mut module_range: Option<SourceRange> = None;

    match argument.kind() {
        "scoped_use_list" => {
            saw_group = true;
            let path = argument.child_by_field_name("path");
            let list = argument.child_by_field_name("list");
            let prefix = path
                .map(|path| builder.text(path).to_string())
                .unwrap_or_default();
            if let Some(path) = path {
                module = Some(prefix.clone());
                module_range = Some(builder.range(path));
            }
            if let Some(list) = list {
                // `module` already carries the shared prefix, so group entries
                // are recorded relative to it.
                emit_use_tree(
                    builder,
                    list,
                    "",
                    &mut items,
                    &mut saw_group,
                    &mut saw_wildcard,
                );
            }
        }
        "use_wildcard" => {
            saw_wildcard = true;
            let mut cursor = argument.walk();
            let path = argument
                .named_children(&mut cursor)
                .find(|child| child.kind() != "use_wildcard");
            if let Some(path) = path {
                module = Some(builder.text(path).to_string());
                module_range = Some(builder.range(path));
            }
            items.push(wildcard_item(builder, argument));
        }
        _ => {
            emit_use_tree(
                builder,
                argument,
                "",
                &mut items,
                &mut saw_group,
                &mut saw_wildcard,
            );
        }
    }

    let form = if saw_group {
        ImportForm::Grouped
    } else if saw_wildcard {
        ImportForm::Wildcard
    } else {
        ImportForm::Single
    };
    builder.push_import(form, module, module_range, None, items, builder.range(node));
}

fn emit_use_tree(
    builder: &mut FactBuilder<'_>,
    node: Node,
    prefix: &str,
    items: &mut Vec<crate::model::ImportItem>,
    saw_group: &mut bool,
    saw_wildcard: &mut bool,
) {
    match node.kind() {
        "use_list" => {
            *saw_group = true;
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                emit_use_tree(builder, child, prefix, items, saw_group, saw_wildcard);
            }
        }
        "scoped_use_list" => {
            *saw_group = true;
            let path = node.child_by_field_name("path");
            let nested_prefix = match path {
                Some(path) => join_prefix(prefix, builder.text(path)),
                None => prefix.to_string(),
            };
            if let Some(list) = node.child_by_field_name("list") {
                emit_use_tree(
                    builder,
                    list,
                    &nested_prefix,
                    items,
                    saw_group,
                    saw_wildcard,
                );
            }
        }
        "use_wildcard" => {
            *saw_wildcard = true;
            items.push(wildcard_item(builder, node));
        }
        "use_as_clause" => {
            let path = node.child_by_field_name("path");
            let alias = node.child_by_field_name("alias");
            if let (Some(path), Some(alias)) = (path, alias) {
                let target = join_prefix(prefix, builder.text(path));
                items.push(import_item(
                    target,
                    builder.range(path),
                    Some((builder.text(alias).to_string(), builder.range(alias))),
                    false,
                    ImportCategory::Normal,
                    builder.range(node),
                ));
            }
        }
        _ => {
            items.push(import_item(
                join_prefix(prefix, builder.text(node)),
                builder.range(node),
                None,
                false,
                ImportCategory::Normal,
                builder.range(node),
            ));
        }
    }
}

fn wildcard_item(builder: &FactBuilder<'_>, node: Node) -> crate::model::ImportItem {
    let range = star_range(builder, node);
    import_item("*", range, None, true, ImportCategory::Normal, range)
}

/// Range of the trailing `*` of a `use_wildcard` node.
fn star_range(builder: &FactBuilder<'_>, node: Node) -> SourceRange {
    let end = node.end_byte() as u32;
    let start = end.saturating_sub(1).max(node.start_byte() as u32);
    builder.range_from_offsets(start, end)
}

fn join_prefix(prefix: &str, part: &str) -> String {
    if prefix.is_empty() {
        part.to_string()
    } else {
        format!("{prefix}::{part}")
    }
}

fn visit_closure(builder: &mut FactBuilder<'_>, node: Node) {
    let range = builder.range(node);
    builder.push_scope(ScopeKind::Closure, None::<String>, range, None);
    emit_closure_param_bindings(builder, node);
    visit_children(builder, node);
    builder.pop_scope();
}

/// Pattern nodes that contain other patterns rather than binding a name
/// directly. Anything not listed and not a binding leaf is treated as a
/// non-binding (literal, path, wildcard, `..`), so an unknown pattern shape
/// under-captures instead of over-capturing a name that is not a binding.
const PATTERN_CONTAINERS: &[&str] = &[
    "tuple_pattern",
    "slice_pattern",
    "or_pattern",
    "parenthesized_pattern",
    "tuple_struct_pattern",
    "struct_pattern",
    "field_pattern",
    "ref_pattern",
    "mut_pattern",
    "reference_pattern",
    "box_pattern",
    "unary_pattern",
    "deref_pattern",
    "conjunction",
    "identifier_pattern",
];

/// Collect the written identifiers bound by a pattern, in source order.
///
/// `refutable` marks whether the pattern sits in a refutable position (a
/// `match` arm, an `if let`/`while let` condition, or a `let`...`else`). A bare
/// `identifier` there is *ambiguous*: syntax alone cannot prove it binds a
/// fresh local rather than referencing a unit variant, constant or path, so it
/// is recorded with `ambiguous = true`. In irrefutable positions (`let`, `for`,
/// parameters) a bare `identifier` is a definite binding (`ambiguous = false`).
/// `shorthand_field_identifier` and `name @ ...` capture names are definite
/// bindings regardless of position.
fn bound_names<'tree>(
    builder: &FactBuilder<'tree>,
    node: Node<'tree>,
    refutable: bool,
    out: &mut Vec<(String, SourceRange, bool)>,
) {
    match node.kind() {
        "identifier" => {
            out.push((
                builder.text(node).to_string(),
                builder.range(node),
                refutable,
            ));
        }
        "shorthand_field_identifier" => {
            out.push((builder.text(node).to_string(), builder.range(node), false));
        }
        "captured_pattern" => {
            // `name @ pat`: the first identifier is the (definite) binding; the
            // remainder is the sub-pattern that is matched, not bound.
            let mut cursor = node.walk();
            let mut children = node.named_children(&mut cursor);
            if let Some(first) = children.next() {
                out.push((builder.text(first).to_string(), builder.range(first), false));
            }
            for child in children {
                bound_names(builder, child, refutable, out);
            }
        }
        kind if PATTERN_CONTAINERS.contains(&kind) => {
            // Recurse into children, skipping the `type` field — the
            // constructor/type name of a struct or tuple-struct pattern, which
            // is a bare `identifier` but never a binding.
            let ty = node.child_by_field_name("type").map(|n| n.id());
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if Some(child.id()) == ty {
                    continue;
                }
                bound_names(builder, child, refutable, out);
            }
        }
        _ => {}
    }
}

/// Emit bindings for each written name a pattern introduces.
fn emit_pattern_bindings(
    builder: &mut FactBuilder<'_>,
    kind: BindingKind,
    pattern: Node,
    site: Node,
    refutable: bool,
    visibility_ranges: Vec<SourceRange>,
) {
    let mut names = Vec::new();
    bound_names(builder, pattern, refutable, &mut names);
    let site_range = builder.range(site);
    for (name, name_range, ambiguous) in names {
        builder.push_binding(
            kind,
            name,
            name_range,
            site_range,
            visibility_ranges.clone(),
            ambiguous,
        );
    }
}

/// `let <pat> = <init>;`. The binding is visible only *after* the statement,
/// so its visibility runs from the end of the `let_declaration` (which includes
/// the `;`) to the end of the enclosing `container` block. The initializer and
/// anything before the `let` are therefore not covered.
fn emit_let_bindings(builder: &mut FactBuilder<'_>, node: Node, container: Node) {
    let Some(pattern) = node.child_by_field_name("pattern") else {
        return;
    };
    // `let`...`else` is the only refutable `let`; a plain `let` is irrefutable.
    let refutable = node.child_by_field_name("alternative").is_some();
    let start = node.end_byte() as u32;
    let end = container.end_byte() as u32;
    if start > end {
        return;
    }
    let visibility = vec![builder.range_from_offsets(start, end)];
    emit_pattern_bindings(
        builder,
        BindingKind::Let,
        pattern,
        node,
        refutable,
        visibility,
    );
}

/// `for <pat> in <iter> { <body> }`. The binding covers the loop body only —
/// not the iterator expression and not the code after the loop.
fn emit_for_bindings(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(pattern) = node.child_by_field_name("pattern") else {
        return;
    };
    let body = node.child_by_field_name("body");
    let visibility = body.map(|b| vec![builder.range(b)]).unwrap_or_default();
    emit_pattern_bindings(
        builder,
        BindingKind::ForPattern,
        pattern,
        pattern,
        false,
        visibility,
    );
}

/// `match` arm `<pat> [if <guard>] => <body>`. The binding covers the guard
/// (when present) and the arm body — two disjoint regions, which is why the
/// model carries `visibility_ranges[]` rather than a single range.
fn emit_match_bindings(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(match_pattern) = node.child_by_field_name("pattern") else {
        return;
    };
    let condition = match_pattern.child_by_field_name("condition");
    // The inner pattern is the named child of `match_pattern` that is not the
    // `condition` guard.
    let cond_id = condition.map(|c| c.id());
    let inner = {
        let mut cursor = match_pattern.walk();
        let mut found = None;
        for child in match_pattern.named_children(&mut cursor) {
            if Some(child.id()) != cond_id {
                found = Some(child);
                break;
            }
        }
        found
    };
    let Some(inner) = inner else {
        return;
    };
    let mut visibility = Vec::new();
    if let Some(condition) = condition {
        visibility.push(builder.range(condition));
    }
    if let Some(value) = node.child_by_field_name("value") {
        visibility.push(builder.range(value));
    }
    emit_pattern_bindings(
        builder,
        BindingKind::MatchPattern,
        inner,
        inner,
        true,
        visibility,
    );
}

/// `if let <pat> = <value> { <consequence> }`. The binding covers the
/// consequence block only — not the initializer `value` and not any `else`
/// alternative. A `let`-chain yields several `let_condition`s; an earlier
/// condition's binding is also usable in later conditions' values, which the
/// `[condition.end, consequence.end]` range covers.
fn emit_if_let_bindings(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(condition) = node.child_by_field_name("condition") else {
        return;
    };
    let mut conditions = Vec::new();
    if condition.kind() == "let_condition" {
        conditions.push(condition);
    } else if condition.kind() == "let_chain" {
        let mut cursor = condition.walk();
        for child in condition.named_children(&mut cursor) {
            if child.kind() == "let_condition" {
                conditions.push(child);
            }
        }
    } else {
        return;
    }
    let consequence_end = node
        .child_by_field_name("consequence")
        .map(|c| c.end_byte() as u32);
    for condition in conditions {
        let Some(pattern) = condition.child_by_field_name("pattern") else {
            continue;
        };
        let start = condition.end_byte() as u32;
        let end = consequence_end.unwrap_or(start);
        if start > end {
            continue;
        }
        let visibility = vec![builder.range_from_offsets(start, end)];
        emit_pattern_bindings(
            builder,
            BindingKind::IfLetPattern,
            pattern,
            condition,
            true,
            visibility,
        );
    }
}

/// `while let <pat> = <value> { <body> }`. The binding covers the loop body
/// only — not the condition `value` and not the code after the loop.
fn emit_while_let_bindings(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(condition) = node.child_by_field_name("condition") else {
        return;
    };
    if condition.kind() != "let_condition" {
        return;
    }
    let Some(pattern) = condition.child_by_field_name("pattern") else {
        return;
    };
    let body = node.child_by_field_name("body");
    let visibility = body.map(|b| vec![builder.range(b)]).unwrap_or_default();
    emit_pattern_bindings(
        builder,
        BindingKind::WhileLetPattern,
        pattern,
        condition,
        true,
        visibility,
    );
}

/// Function/method `parameter` bindings. `body_range` is `None` for a
/// signature-only item (`fn f(x: T);`), so the parameter still binds a name but
/// covers nothing. `self` receivers are not emitted: `self` is a receiver, not
/// a written name that can be a plain-name callee.
fn emit_parameter_bindings(
    builder: &mut FactBuilder<'_>,
    node: Node,
    body_range: Option<SourceRange>,
) {
    let Some(parameters) = node.child_by_field_name("parameters") else {
        return;
    };
    let visibility = body_range.map(|r| vec![r]).unwrap_or_default();
    let mut cursor = parameters.walk();
    for param in parameters.children(&mut cursor) {
        if param.kind() != "parameter" {
            continue;
        }
        if let Some(pattern) = param.child_by_field_name("pattern") {
            emit_pattern_bindings(
                builder,
                BindingKind::FunctionParameter,
                pattern,
                param,
                false,
                visibility.clone(),
            );
        }
    }
}

/// Closure parameter bindings. `closure_parameters` children are `parameter`
/// nodes when typed (`|x: u8|`) or bare pattern nodes otherwise (`|x|`,
/// `|(a, b)|`). The binding covers the closure body only.
fn emit_closure_param_bindings(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(parameters) = node.child_by_field_name("parameters") else {
        return;
    };
    let body = node.child_by_field_name("body");
    let visibility = body.map(|b| vec![builder.range(b)]).unwrap_or_default();
    let mut cursor = parameters.walk();
    for param in parameters.children(&mut cursor) {
        let pattern = if param.kind() == "parameter" {
            param.child_by_field_name("pattern")
        } else {
            Some(param)
        };
        if let Some(pattern) = pattern {
            emit_pattern_bindings(
                builder,
                BindingKind::ClosureParameter,
                pattern,
                param,
                false,
                visibility.clone(),
            );
        }
    }
}

fn emit_call(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    let (form, callee, type_arguments) = classify_callee(builder, function);
    builder.push_call(
        form,
        callee,
        builder.range(function),
        builder.range(node),
        type_arguments,
        false,
        false,
    );
}

fn classify_callee<'tree>(
    builder: &FactBuilder<'tree>,
    function: Node<'tree>,
) -> (CallLikeForm, String, Option<(String, SourceRange)>) {
    match function.kind() {
        "identifier" => (
            CallLikeForm::PlainName,
            builder.text(function).to_string(),
            None,
        ),
        "scoped_identifier" => (
            CallLikeForm::QualifiedPath,
            builder.text(function).to_string(),
            None,
        ),
        "field_expression" => (
            CallLikeForm::MemberSelector,
            builder.text(function).to_string(),
            None,
        ),
        "generic_function" => {
            let type_arguments = function
                .child_by_field_name("type_arguments")
                .map(|args| (builder.text(args).to_string(), builder.range(args)));
            let inner_form = function
                .child_by_field_name("function")
                .map(|inner| match inner.kind() {
                    "scoped_identifier" => CallLikeForm::QualifiedPath,
                    "field_expression" => CallLikeForm::MemberSelector,
                    _ => CallLikeForm::PlainName,
                })
                .unwrap_or(CallLikeForm::PlainName);
            (
                inner_form,
                builder.text(function).to_string(),
                type_arguments,
            )
        }
        _ => (
            CallLikeForm::Indirect,
            builder.text(function).to_string(),
            None,
        ),
    }
}

fn emit_macro_invocation(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(macro_node) = node.child_by_field_name("macro") else {
        return;
    };
    builder.push_call(
        CallLikeForm::MacroInvocation,
        builder.text(macro_node),
        builder.range(macro_node),
        builder.range(node),
        None,
        false,
        false,
    );
}

/// Range of the construct header: everything before its body.
fn header_range(builder: &FactBuilder<'_>, node: Node, body: Node) -> Option<SourceRange> {
    let start = node.start_byte() as u32;
    let end = body.start_byte() as u32;
    if end <= start {
        return None;
    }
    Some(builder.range_from_offsets(start, end))
}
