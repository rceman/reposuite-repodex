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
    CallLikeForm, DeclarationFlag, DeclarationKind, ImportCategory, ImportForm, LanguageId,
    ReferenceKind, ScopeKind, SourceRange, TestEvidence, TestEvidenceKind,
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
        visit_item_list(builder, body);
        builder.pop_scope();
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
    visit_children(builder, node);
    builder.pop_scope();
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
