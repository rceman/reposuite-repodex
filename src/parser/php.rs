//! PHP language adapter (modern PHP, mixed HTML/PHP source supported).
//!
//! Boundaries:
//!
//! * `use Foo\Bar;` (namespace import), `use TraitName;` inside a class body
//!   (trait composition) and `function () use ($x) {}` (closure capture) are
//!   three different node kinds and are never conflated;
//! * first-class callable syntax `foo(...)` is *not* an invocation. It is
//!   recorded as a `first_class_callable` reference;
//! * no Laravel/Lumen or any other framework semantics are claimed;
//! * trait adaptation bodies are not extracted in TASK 1.

use tree_sitter::{Language, Node, Tree};

use crate::model::{
    CallLikeForm, DeclarationFlag, DeclarationKind, ImportCategory, ImportForm, LanguageId,
    ReceiverEvidenceKind, ReferenceKind, ScopeKind, SourceRange, TestEvidence, TestEvidenceKind,
};

use super::builder::{import_item, DeclarationDraft, FactBuilder};
use super::recovery::RecoveryScanner;
use super::{AdapterError, GrammarInfo, LanguageAdapter};

pub struct PhpAdapter {
    recovery: RecoveryScanner,
}

impl PhpAdapter {
    pub fn new() -> Result<Self, AdapterError> {
        // `LANGUAGE_PHP` is the mixed HTML/PHP grammar, which is what an
        // ordinary `.php` file needs. `LANGUAGE_PHP_ONLY` would reject a file
        // that contains any markup outside `<?php ... ?>`.
        let language: Language = tree_sitter_php::LANGUAGE_PHP.into();
        let recovery =
            RecoveryScanner::new(LanguageId::Php, &language).map_err(|message| AdapterError {
                language: Some(LanguageId::Php),
                message,
            })?;
        Ok(Self { recovery })
    }
}

const GRAMMAR: GrammarInfo = GrammarInfo {
    crate_name: "tree-sitter-php",
    crate_version: "0.24.2",
    upstream_repository: "https://github.com/tree-sitter/tree-sitter-php",
    license: "MIT",
    upstream_queries: &[
        "queries/highlights.scm",
        "queries/injections.scm",
        "queries/injections-text.scm",
    ],
    repo_queries: &["queries/php/recovery.scm"],
    root_node_kind: "program",
    tree_sitter_compatibility: "ABI 15 (tree-sitter 0.27.x)",
};

impl LanguageAdapter for PhpAdapter {
    fn language(&self) -> LanguageId {
        LanguageId::Php
    }

    fn grammar(&self) -> GrammarInfo {
        GRAMMAR
    }

    fn ts_language(&self) -> Language {
        tree_sitter_php::LANGUAGE_PHP.into()
    }

    fn recovery(&self) -> &RecoveryScanner {
        &self.recovery
    }

    fn extract(&self, builder: &mut FactBuilder<'_>, tree: &Tree) {
        visit_program(builder, tree.root_node());
    }
}

/// `program` is special: an unbracketed `namespace Foo;` keeps its scope open
/// for every following sibling until the next namespace declaration or EOF.
fn visit_program(builder: &mut FactBuilder<'_>, program: Node) {
    let mut open_namespace = false;
    let mut cursor = program.walk();
    for child in program.children(&mut cursor) {
        if child.kind() == "namespace_definition" {
            if open_namespace {
                builder.pop_scope();
            }
            open_namespace = handle_namespace(builder, child);
            continue;
        }
        if !child.is_named() {
            continue;
        }
        visit(builder, child);
    }
    if open_namespace {
        builder.pop_scope();
    }
}

fn visit(builder: &mut FactBuilder<'_>, node: Node) {
    match node.kind() {
        "namespace_definition" => {
            let open = handle_namespace(builder, node);
            if open {
                // A bracketed-free namespace nested somewhere unexpected: close
                // it immediately so the scope stack stays balanced.
                builder.pop_scope();
            }
        }
        "namespace_use_declaration" => handle_namespace_use(builder, node),
        "class_declaration" => {
            handle_class_like(builder, node, DeclarationKind::Class, ScopeKind::Class)
        }
        "interface_declaration" => handle_class_like(
            builder,
            node,
            DeclarationKind::Interface,
            ScopeKind::Interface,
        ),
        "trait_declaration" => {
            handle_class_like(builder, node, DeclarationKind::Trait, ScopeKind::Trait)
        }
        "enum_declaration" => handle_enum(builder, node),
        "enum_case" => handle_enum_case(builder, node),
        "function_definition" => handle_function(builder, node),
        "method_declaration" => handle_method(builder, node),
        "property_declaration" => handle_property(builder, node),
        "const_declaration" => handle_const(builder, node),
        "use_declaration" => handle_trait_use(builder, node),
        "anonymous_function" => handle_anonymous_function(builder, node),
        "arrow_function" => handle_arrow_function(builder, node),
        "function_call_expression"
        | "member_call_expression"
        | "nullsafe_member_call_expression"
        | "scoped_call_expression" => {
            emit_call(builder, node);
            visit_children(builder, node);
        }
        "object_creation_expression" => {
            emit_construction(builder, node);
            visit_children(builder, node);
        }
        "include_expression"
        | "include_once_expression"
        | "require_expression"
        | "require_once_expression" => {
            emit_include(builder, node);
            visit_children(builder, node);
        }
        "assignment_expression" => {
            emit_receiver_write(builder, node);
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

/// Returns `true` when the namespace is unbracketed and its scope must stay
/// open for the following siblings.
fn handle_namespace(builder: &mut FactBuilder<'_>, node: Node) -> bool {
    let name_node = node.child_by_field_name("name");
    let body = node.child_by_field_name("body");
    let name = name_node.map(|node| builder.text(node).to_string());
    let range = builder.range(node);
    if let Some(name_node) = name_node {
        builder.push_declaration(DeclarationDraft::new(
            DeclarationKind::Namespace,
            builder.text(name_node).to_string(),
            builder.range(name_node),
            range,
        ));
    }
    let header = body.and_then(|body| header_range(builder, node, body));
    builder.push_scope(ScopeKind::Namespace, name, range, header);
    match body {
        Some(body) => {
            visit_children(builder, body);
            builder.pop_scope();
            false
        }
        None => true,
    }
}

fn handle_namespace_use(builder: &mut FactBuilder<'_>, node: Node) {
    let declaration_category = category_of(builder, node);
    let mut items = Vec::new();
    let mut module: Option<String> = None;
    let mut module_range: Option<SourceRange> = None;
    let mut grouped = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "namespace_name" => {
                module = Some(builder.text(child).to_string());
                module_range = Some(builder.range(child));
            }
            "namespace_use_clause" => {
                items.push(clause_item(builder, child, declaration_category));
            }
            "namespace_use_group" => {
                grouped = true;
                let mut inner = child.walk();
                for clause in child.children(&mut inner) {
                    if clause.kind() == "namespace_use_clause" {
                        items.push(clause_item(builder, clause, declaration_category));
                    }
                }
            }
            _ => {}
        }
    }
    let form = if grouped {
        ImportForm::Grouped
    } else {
        ImportForm::Single
    };
    builder.push_import(form, module, module_range, None, items, builder.range(node));
}

fn clause_item(
    builder: &FactBuilder<'_>,
    clause: Node,
    declaration_category: Option<ImportCategory>,
) -> crate::model::ImportItem {
    let alias_node = clause.child_by_field_name("alias");
    let alias_id = alias_node.map(|node| node.id());
    let mut target_node: Option<Node> = None;
    let mut cursor = clause.walk();
    for child in clause.children(&mut cursor) {
        if !child.is_named() || Some(child.id()) == alias_id {
            continue;
        }
        if matches!(child.kind(), "name" | "qualified_name" | "relative_name") {
            target_node = Some(child);
        }
    }
    let target = target_node
        .map(|node| builder.text(node).to_string())
        .unwrap_or_default();
    let target_range = target_node
        .map(|node| builder.range(node))
        .unwrap_or_else(|| builder.range(clause));
    let category = category_of(builder, clause)
        .or(declaration_category)
        .unwrap_or(ImportCategory::Normal);
    import_item(
        target,
        target_range,
        alias_node.map(|node| (builder.text(node).to_string(), builder.range(node))),
        false,
        category,
        builder.range(clause),
    )
}

/// `use function ...` / `use const ...` category, read from the `type` field.
fn category_of(builder: &FactBuilder<'_>, node: Node) -> Option<ImportCategory> {
    let mut cursor = node.walk();
    for (index, child) in node.children(&mut cursor).enumerate() {
        if node.field_name_for_child(index as u32) != Some("type") {
            continue;
        }
        match child.kind() {
            "function" => return Some(ImportCategory::Function),
            "const" => return Some(ImportCategory::Constant),
            _ => {}
        }
    }
    let _ = builder;
    None
}

fn handle_class_like(
    builder: &mut FactBuilder<'_>,
    node: Node,
    kind: DeclarationKind,
    scope_kind: ScopeKind,
) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = builder.text(name_node).to_string();
    let range = builder.range(node);
    let body = node.child_by_field_name("body");
    let mut draft = DeclarationDraft::new(kind, name.clone(), builder.range(name_node), range)
        .with_body(body.map(|body| builder.range(body)));
    draft = apply_modifiers(builder, node, draft);
    let declaration_id = builder.push_declaration(draft);

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "base_clause" => {
                for target in clause_names(builder, child) {
                    builder.push_reference(
                        ReferenceKind::BaseClass,
                        builder.text(target),
                        builder.range(target),
                        Some(declaration_id),
                    );
                }
            }
            "class_interface_clause" => {
                for target in clause_names(builder, child) {
                    builder.push_reference(
                        ReferenceKind::ImplementedInterface,
                        builder.text(target),
                        builder.range(target),
                        Some(declaration_id),
                    );
                }
            }
            "attribute_list" => {
                if let Some(evidence) = attribute_test_evidence(builder, child, &["Test"]) {
                    builder.add_test_evidence(declaration_id, evidence);
                }
            }
            _ => {}
        }
    }

    if let Some(body) = body {
        let header = header_range(builder, node, body);
        builder.push_scope(scope_kind, Some(name), range, header);
        visit_children(builder, body);
        builder.pop_scope();
    }
}

fn handle_enum(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = builder.text(name_node).to_string();
    let range = builder.range(node);
    let body = node.child_by_field_name("body");
    let declaration_id = builder.push_declaration(
        DeclarationDraft::new(
            DeclarationKind::Enum,
            name.clone(),
            builder.range(name_node),
            range,
        )
        .with_body(body.map(|body| builder.range(body))),
    );
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "class_interface_clause" {
            for target in clause_names(builder, child) {
                builder.push_reference(
                    ReferenceKind::ImplementedInterface,
                    builder.text(target),
                    builder.range(target),
                    Some(declaration_id),
                );
            }
        }
    }
    if let Some(body) = body {
        let header = header_range(builder, node, body);
        builder.push_scope(ScopeKind::Enum, Some(name), range, header);
        visit_children(builder, body);
        builder.pop_scope();
    }
}

/// `enum_case` appears inside an enum body.
fn handle_enum_case(builder: &mut FactBuilder<'_>, node: Node) {
    let mut cursor = node.walk();
    let name_node = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "name");
    let Some(name_node) = name_node else {
        return;
    };
    builder.push_declaration(DeclarationDraft::new(
        DeclarationKind::Variant,
        builder.text(name_node).to_string(),
        builder.range(name_node),
        builder.range(node),
    ));
}

fn clause_names<'tree>(builder: &FactBuilder<'_>, clause: Node<'tree>) -> Vec<Node<'tree>> {
    let mut names = Vec::new();
    let mut cursor = clause.walk();
    for child in clause.children(&mut cursor) {
        if matches!(child.kind(), "name" | "qualified_name" | "relative_name") {
            names.push(child);
        }
    }
    let _ = builder;
    names
}

fn handle_function(builder: &mut FactBuilder<'_>, node: Node) {
    emit_default_value_expressions(builder, node);
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = builder.text(name_node).to_string();
    let range = builder.range(node);
    let body = node.child_by_field_name("body");
    let declaration_id = builder.push_declaration(
        DeclarationDraft::new(
            DeclarationKind::Function,
            name.clone(),
            builder.range(name_node),
            range,
        )
        .with_body(body.map(|body| builder.range(body))),
    );
    emit_php_test_evidence(builder, node, name_node, &name, declaration_id);
    if let Some(body) = body {
        builder.push_scope(ScopeKind::Function, Some(name), range, None);
        if let Some(parameters) = node.child_by_field_name("parameters") {
            emit_parameter_type_annotations(builder, parameters);
        }
        visit_children(builder, body);
        builder.pop_scope();
    }
}

fn handle_method(builder: &mut FactBuilder<'_>, node: Node) {
    emit_default_value_expressions(builder, node);
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = builder.text(name_node).to_string();
    let range = builder.range(node);
    let body = node.child_by_field_name("body");
    let mut draft = DeclarationDraft::new(
        DeclarationKind::Method,
        name.clone(),
        builder.range(name_node),
        range,
    )
    .with_body(body.map(|body| builder.range(body)));
    draft = apply_modifiers(builder, node, draft);
    if name == "__construct" {
        draft = draft.with_flag(DeclarationFlag::Constructor);
    }
    let declaration_id = builder.push_declaration(draft);
    emit_php_test_evidence(builder, node, name_node, &name, declaration_id);

    // Promoted constructor properties belong to the enclosing class scope, so
    // they are emitted before the method scope is pushed.
    if let Some(parameters) = node.child_by_field_name("parameters") {
        emit_promoted_properties(builder, parameters);
    }

    if let Some(body) = body {
        builder.push_scope(ScopeKind::Method, Some(name), range, None);
        if let Some(parameters) = node.child_by_field_name("parameters") {
            emit_parameter_type_annotations(builder, parameters);
        }
        visit_children(builder, body);
        builder.pop_scope();
    }
}

/// PHP evaluates parameter default expressions when the call happens, but the
/// written syntax sits in the parameter list; call-like occurrences there are
/// attributed to the enclosing scope so that a method body stays readable.
fn emit_default_value_expressions(builder: &mut FactBuilder<'_>, node: Node) {
    if let Some(parameters) = node.child_by_field_name("parameters") {
        visit(builder, parameters);
    }
}

fn emit_promoted_properties(builder: &mut FactBuilder<'_>, parameters: Node) {
    let mut cursor = parameters.walk();
    for child in parameters.children(&mut cursor) {
        if child.kind() != "property_promotion_parameter" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let Some(identifier) = inner_variable_name(builder, name_node) else {
            continue;
        };
        let mut draft = DeclarationDraft::new(
            DeclarationKind::Property,
            builder.text(identifier).to_string(),
            builder.range(identifier),
            builder.range(child),
        )
        .with_flag(DeclarationFlag::Promoted);
        if has_child_kind(child, "readonly_modifier") {
            draft = draft.with_flag(DeclarationFlag::Readonly);
        }
        builder.push_declaration(draft);
        // A promoted property is a class-scoped type hint on `$this->name`.
        if let Some(type_node) = child.child_by_field_name("type") {
            builder.push_receiver_evidence(
                ReceiverEvidenceKind::PropertyTypeHint,
                format!("$this->{}", builder.text(identifier)),
                builder.range(identifier),
                builder.text(type_node),
                builder.range(type_node),
                builder.range(child),
            );
        }
    }
}

fn handle_property(builder: &mut FactBuilder<'_>, node: Node) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "property_element" {
            continue;
        }
        let mut inner = child.walk();
        let Some(variable) = child
            .children(&mut inner)
            .find(|element| element.kind() == "variable_name")
        else {
            continue;
        };
        let Some(identifier) = inner_variable_name(builder, variable) else {
            continue;
        };
        let mut draft = DeclarationDraft::new(
            DeclarationKind::Property,
            builder.text(identifier).to_string(),
            builder.range(identifier),
            builder.range(child),
        );
        draft = apply_modifiers(builder, node, draft);
        builder.push_declaration(draft);
        if let Some(type_node) = node.child_by_field_name("type") {
            builder.push_receiver_evidence(
                ReceiverEvidenceKind::PropertyTypeHint,
                format!("$this->{}", builder.text(identifier)),
                builder.range(identifier),
                builder.text(type_node),
                builder.range(type_node),
                builder.range(child),
            );
        }
    }
}

fn handle_const(builder: &mut FactBuilder<'_>, node: Node) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "const_element" {
            continue;
        }
        let mut inner = child.walk();
        let name_node = child
            .named_children(&mut inner)
            .find(|element| element.kind() == "name");
        let Some(name_node) = name_node else {
            continue;
        };
        let mut draft = DeclarationDraft::new(
            DeclarationKind::Constant,
            builder.text(name_node).to_string(),
            builder.range(name_node),
            builder.range(child),
        );
        draft = apply_modifiers(builder, node, draft);
        builder.push_declaration(draft);
    }
}

/// `use TraitA, TraitB;` inside a class body. Never a namespace import.
fn handle_trait_use(builder: &mut FactBuilder<'_>, node: Node) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "name" | "qualified_name" | "relative_name") {
            builder.push_reference(
                ReferenceKind::TraitComposition,
                builder.text(child),
                builder.range(child),
                None,
            );
        }
    }
}

fn handle_anonymous_function(builder: &mut FactBuilder<'_>, node: Node) {
    let range = builder.range(node);
    builder.push_scope(ScopeKind::Closure, None::<String>, range, None);
    if let Some(parameters) = node.child_by_field_name("parameters") {
        emit_parameter_type_annotations(builder, parameters);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "anonymous_function_use_clause" {
            emit_closure_captures(builder, child);
        }
    }
    visit_children(builder, node);
    builder.pop_scope();
}

fn emit_closure_captures(builder: &mut FactBuilder<'_>, clause: Node) {
    let mut cursor = clause.walk();
    for child in clause.children(&mut cursor) {
        match child.kind() {
            "variable_name" => {
                builder.push_reference(
                    ReferenceKind::ClosureCapture,
                    builder.text(child),
                    builder.range(child),
                    None,
                );
            }
            "by_ref" => {
                builder.push_reference(
                    ReferenceKind::ClosureCapture,
                    builder.text(child),
                    builder.range(child),
                    None,
                );
            }
            _ => {}
        }
    }
}

fn handle_arrow_function(builder: &mut FactBuilder<'_>, node: Node) {
    let range = builder.range(node);
    builder.push_scope(ScopeKind::ArrowFunction, None::<String>, range, None);
    if let Some(parameters) = node.child_by_field_name("parameters") {
        emit_parameter_type_annotations(builder, parameters);
    }
    visit_children(builder, node);
    builder.pop_scope();
}

fn apply_modifiers(
    builder: &FactBuilder<'_>,
    node: Node,
    mut draft: DeclarationDraft,
) -> DeclarationDraft {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "static_modifier" => draft = draft.with_flag(DeclarationFlag::Static),
            "abstract_modifier" => draft = draft.with_flag(DeclarationFlag::Abstract),
            "final_modifier" => draft = draft.with_flag(DeclarationFlag::Final),
            "readonly_modifier" => draft = draft.with_flag(DeclarationFlag::Readonly),
            "variadic_parameter" | "variadic_placeholder" => {
                draft = draft.with_flag(DeclarationFlag::Variadic)
            }
            _ => {}
        }
    }
    let _ = builder;
    draft
}

fn has_child_kind(node: Node, kind: &str) -> bool {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).any(|child| child.kind() == kind);
    found
}

fn inner_variable_name<'tree>(
    builder: &FactBuilder<'_>,
    variable: Node<'tree>,
) -> Option<Node<'tree>> {
    if variable.kind() == "name" {
        return Some(variable);
    }
    let mut cursor = variable.walk();
    let found = variable
        .named_children(&mut cursor)
        .find(|child| child.kind() == "name");
    let _ = builder;
    found
}

/// PHP test evidence:
///
/// * an attribute whose last written segment is `Test` (PHPUnit's attribute);
/// * a method/function name starting with `test`.
fn emit_php_test_evidence(
    builder: &mut FactBuilder<'_>,
    node: Node,
    name_node: Node,
    name: &str,
    declaration_id: u32,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "attribute_list" {
            continue;
        }
        if let Some(evidence) = attribute_test_evidence(builder, child, &["Test"]) {
            builder.add_test_evidence(declaration_id, evidence);
        }
    }
    if name.starts_with("test") {
        builder.add_test_evidence(
            declaration_id,
            TestEvidence {
                kind: TestEvidenceKind::FunctionNameConvention,
                detail: "test prefix".to_string(),
                range: builder.range(name_node),
            },
        );
    }
}

fn attribute_test_evidence(
    builder: &FactBuilder<'_>,
    attribute_list: Node,
    markers: &[&str],
) -> Option<TestEvidence> {
    let mut cursor = attribute_list.walk();
    for group in attribute_list.children(&mut cursor) {
        if group.kind() != "attribute_group" {
            continue;
        }
        let mut inner = group.walk();
        for attribute in group.children(&mut inner) {
            if attribute.kind() != "attribute" {
                continue;
            }
            let mut attribute_cursor = attribute.walk();
            let name_node = attribute
                .named_children(&mut attribute_cursor)
                .find(|child| matches!(child.kind(), "name" | "qualified_name" | "relative_name"));
            let Some(name_node) = name_node else {
                continue;
            };
            let written = builder.text(name_node);
            let last_segment = written.rsplit('\\').next().unwrap_or(written);
            if markers.contains(&last_segment) {
                return Some(TestEvidence {
                    kind: TestEvidenceKind::Attribute,
                    detail: format!("#[{written}]"),
                    range: builder.range(attribute),
                });
            }
        }
    }
    None
}

/// Emit `parameter_type_hint` receiver evidence for every typed parameter.
/// Runs inside the pushed callable scope so `scope_id` is the callable scope
/// the parameter is visible in. Untyped parameters emit nothing — absence of
/// a hint is itself a fact (the receiver stays untyped).
fn emit_parameter_type_annotations(builder: &mut FactBuilder<'_>, parameters: Node) {
    let mut cursor = parameters.walk();
    for child in parameters.children(&mut cursor) {
        let Some(type_node) = child.child_by_field_name("type") else {
            continue;
        };
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        if inner_variable_name(builder, name_node).is_none() {
            continue;
        }
        builder.push_receiver_evidence(
            ReceiverEvidenceKind::ParameterTypeHint,
            builder.text(name_node).to_string(),
            builder.range(name_node),
            builder.text(type_node),
            builder.range(type_node),
            builder.range(child),
        );
    }
}

/// `$x = <expr>` / `$this->p = <expr>` — a direct write to a simple receiver
/// slot. Literal `new` right-hand sides carry the written class name; every
/// other RHS is an opaque write that (per the candidate rule's last-write
/// boundary) invalidates earlier local evidence. Writes whose receiver is not
/// a bare variable or a literal `$this->prop` access are ignored: subscripts
/// and property writes do not change the receiver's own type.
fn emit_receiver_write(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };
    let (receiver, kind) = if left.kind() == "variable_name" {
        (builder.text(left).to_string(), false)
    } else if left.kind() == "member_access_expression" {
        let is_this_prop = left
            .child_by_field_name("object")
            .is_some_and(|o| o.kind() == "variable_name" && builder.text(o) == "$this")
            && left
                .child_by_field_name("name")
                .is_some_and(|n| n.kind() == "name");
        if !is_this_prop {
            return;
        }
        (builder.text(left).to_string(), true)
    } else {
        return;
    };
    // Literal `new` right side: the class-name child of the creation node.
    let mut literal_class: Option<Node> = None;
    if right.kind() == "object_creation_expression" {
        let mut cursor = right.walk();
        for child in right.children(&mut cursor) {
            if !child.is_named() || child.kind() == "arguments" {
                continue;
            }
            if matches!(child.kind(), "name" | "qualified_name" | "relative_name") {
                literal_class = Some(child);
            }
            break;
        }
    }
    let (kind, written, written_range) = if let Some(class) = literal_class {
        (
            if kind {
                ReceiverEvidenceKind::PropertyLiteralNew
            } else {
                ReceiverEvidenceKind::LocalLiteralNew
            },
            builder.text(class).to_string(),
            builder.range(class),
        )
    } else {
        (
            if kind {
                ReceiverEvidenceKind::PropertyOpaqueWrite
            } else {
                ReceiverEvidenceKind::LocalOpaqueWrite
            },
            builder.text(right).to_string(),
            builder.range(right),
        )
    };
    builder.push_receiver_evidence(
        kind,
        receiver,
        builder.range(left),
        written,
        written_range,
        builder.range(node),
    );
}

/// `foo(...)` is first-class callable creation, not an invocation.
fn is_first_class_callable(arguments: Node) -> bool {
    let mut cursor = arguments.walk();
    let mut placeholders = 0usize;
    let mut others = 0usize;
    for child in arguments.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if child.kind() == "variadic_placeholder" {
            placeholders += 1;
        } else {
            others += 1;
        }
    }
    placeholders == 1 && others == 0
}

fn emit_call(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return;
    };
    let (callee_range, callee_written) = callee_span(builder, node, arguments);

    if is_first_class_callable(arguments) {
        builder.push_reference(
            ReferenceKind::FirstClassCallable,
            callee_written,
            callee_range,
            None,
        );
        return;
    }

    let (form, dynamic_callee, nullsafe) = match node.kind() {
        "function_call_expression" => {
            let dynamic = node
                .child_by_field_name("function")
                .map(|function| {
                    !matches!(function.kind(), "name" | "qualified_name" | "relative_name")
                })
                .unwrap_or(false);
            let form = match node.child_by_field_name("function").map(|node| node.kind()) {
                Some("name") => CallLikeForm::PlainName,
                Some("qualified_name") | Some("relative_name") => CallLikeForm::QualifiedPath,
                _ => CallLikeForm::Indirect,
            };
            (form, dynamic, false)
        }
        "member_call_expression" => {
            let dynamic = node
                .child_by_field_name("name")
                .map(|name| name.kind() != "name")
                .unwrap_or(false);
            (CallLikeForm::MemberSelector, dynamic, false)
        }
        "nullsafe_member_call_expression" => {
            let dynamic = node
                .child_by_field_name("name")
                .map(|name| name.kind() != "name")
                .unwrap_or(false);
            (CallLikeForm::MemberSelector, dynamic, true)
        }
        "scoped_call_expression" => {
            // Both sides must be literal: the scope (`Foo::`) AND the member
            // name (`::bar`). `Foo::{$m}()` has a literal scope but a dynamic
            // selector — marking only the scope would fabricate a literal
            // method identity the source does not provide.
            let scope_dynamic = node
                .child_by_field_name("scope")
                .map(|scope| {
                    !matches!(
                        scope.kind(),
                        "name" | "qualified_name" | "relative_name" | "relative_scope"
                    )
                })
                .unwrap_or(false);
            let name_dynamic = node
                .child_by_field_name("name")
                .map(|name| name.kind() != "name")
                .unwrap_or(false);
            (
                CallLikeForm::StaticScoped,
                scope_dynamic || name_dynamic,
                false,
            )
        }
        _ => (CallLikeForm::Indirect, false, false),
    };

    builder.push_call(
        form,
        callee_written,
        callee_range,
        builder.range(node),
        None,
        dynamic_callee,
        nullsafe,
    );
}

fn emit_construction(builder: &mut FactBuilder<'_>, node: Node) {
    let mut cursor = node.walk();
    let mut target: Option<Node> = None;
    for child in node.children(&mut cursor) {
        if !child.is_named() || child.kind() == "arguments" {
            continue;
        }
        target = Some(child);
        break;
    }
    let Some(target) = target else {
        return;
    };
    let dynamic = !matches!(target.kind(), "name" | "qualified_name" | "relative_name");
    // An anonymous class has no name, so the written callee is the `class`
    // keyword alone. The `anonymous_class` node spans everything from any
    // leading attribute list through the whole `class ... { ... }` body, so its
    // range would make a five-byte callee claim the entire declaration. The
    // range must cover exactly the keyword token.
    let (written, callee_range) = if target.kind() == "anonymous_class" {
        let mut cursor = target.walk();
        let Some(keyword) = target
            .children(&mut cursor)
            .find(|child| child.kind() == "class")
        else {
            return;
        };
        ("class".to_string(), builder.range(keyword))
    } else {
        (builder.text(target).to_string(), builder.range(target))
    };
    builder.push_call(
        CallLikeForm::ExplicitConstruction,
        written,
        callee_range,
        builder.range(node),
        None,
        dynamic,
        false,
    );
}

fn emit_include(builder: &mut FactBuilder<'_>, node: Node) {
    let mut cursor = node.walk();
    let target = node
        .named_children(&mut cursor)
        .find(|child| child.kind() != "comment");
    let Some(target) = target else {
        return;
    };
    builder.push_reference(
        ReferenceKind::IncludeTarget,
        builder.text(target),
        builder.range(target),
        None,
    );
}

/// Range and written text of the callee: everything between the start of the
/// call node and the opening parenthesis of its argument list.
fn callee_span(builder: &FactBuilder<'_>, node: Node, arguments: Node) -> (SourceRange, String) {
    let source = builder.source();
    let start = node.start_byte() as u32;
    let mut end = arguments.start_byte() as u32;
    while end > start && matches!(source[(end - 1) as usize], b' ' | b'\t' | b'\n' | b'\r') {
        end -= 1;
    }
    let range = builder.range_from_offsets(start, end);
    (range, builder.text_range(range).to_string())
}

fn header_range(builder: &FactBuilder<'_>, node: Node, body: Node) -> Option<SourceRange> {
    let start = node.start_byte() as u32;
    let end = body.start_byte() as u32;
    if end <= start {
        return None;
    }
    Some(builder.range_from_offsets(start, end))
}
