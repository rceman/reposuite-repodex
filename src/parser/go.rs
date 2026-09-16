//! Go language adapter.
//!
//! Boundaries:
//!
//! * a method is declared at file scope with a receiver; it is never nested
//!   inside the receiver type. Receiver syntax is recorded separately;
//! * build tags are never evaluated;
//! * selectors are never resolved and promoted methods are never inferred;
//! * `t.Run` is never treated as proof of test ownership;
//! * `T(x)` is not classified as a call. Tree-sitter's Go grammar already
//!   splits that shape into `type_conversion_expression` (one argument) and
//!   `call_expression` (otherwise), so RepoDex reports what the grammar says
//!   and documents the ambiguity instead of guessing.

use tree_sitter::{Language, Node, Tree};

use crate::model::{
    CallLikeForm, DeclarationFlag, DeclarationKind, ImportCategory, ImportForm, LanguageId,
    ReferenceKind, ScopeKind, SourceRange, TestEvidence, TestEvidenceKind,
};

use super::builder::{import_item, DeclarationDraft, FactBuilder};
use super::recovery::RecoveryScanner;
use super::{AdapterError, GrammarInfo, LanguageAdapter};

pub struct GoAdapter {
    recovery: RecoveryScanner,
}

impl GoAdapter {
    pub fn new() -> Result<Self, AdapterError> {
        let language: Language = tree_sitter_go::LANGUAGE.into();
        let recovery =
            RecoveryScanner::new(LanguageId::Go, &language).map_err(|message| AdapterError {
                language: Some(LanguageId::Go),
                message,
            })?;
        Ok(Self { recovery })
    }
}

const GRAMMAR: GrammarInfo = GrammarInfo {
    crate_name: "tree-sitter-go",
    crate_version: "0.25.0",
    upstream_repository: "https://github.com/tree-sitter/tree-sitter-go",
    license: "MIT",
    upstream_queries: &["queries/tags.scm", "queries/highlights.scm"],
    repo_queries: &["queries/go/recovery.scm"],
    root_node_kind: "source_file",
    tree_sitter_compatibility: "ABI 15 (tree-sitter 0.27.x)",
};

impl LanguageAdapter for GoAdapter {
    fn language(&self) -> LanguageId {
        LanguageId::Go
    }

    fn grammar(&self) -> GrammarInfo {
        GRAMMAR
    }

    fn ts_language(&self) -> Language {
        tree_sitter_go::LANGUAGE.into()
    }

    fn recovery(&self) -> &RecoveryScanner {
        &self.recovery
    }

    fn extract(&self, builder: &mut FactBuilder<'_>, tree: &Tree) {
        emit_file_evidence(builder);
        visit_children(builder, tree.root_node());
    }
}

/// `*_test.go` is a file naming convention only. It never marks a declaration
/// as a test by itself, and RepoDex never claims that `go test` would collect it.
fn emit_file_evidence(builder: &mut FactBuilder<'_>) {
    let path = builder.file().relative_path.clone();
    if path.ends_with("_test.go") {
        let range = builder.range_from_offsets(0, 0);
        builder.add_file_test_evidence(TestEvidence {
            kind: TestEvidenceKind::FileNameConvention,
            detail: "_test.go suffix".to_string(),
            range,
        });
    }
}

fn visit(builder: &mut FactBuilder<'_>, node: Node) {
    match node.kind() {
        "package_clause" => handle_package(builder, node),
        "import_declaration" => handle_import(builder, node),
        "type_declaration" => handle_type_declaration(builder, node),
        "const_declaration" => handle_value_declaration(builder, node, DeclarationKind::Constant),
        "var_declaration" => handle_value_declaration(builder, node, DeclarationKind::Variable),
        "function_declaration" => handle_function(builder, node),
        "method_declaration" => handle_method(builder, node),
        "func_literal" => handle_func_literal(builder, node),
        "call_expression" => {
            emit_call(builder, node);
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

fn handle_package(builder: &mut FactBuilder<'_>, node: Node) {
    let mut cursor = node.walk();
    let name_node = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "package_identifier");
    let Some(name_node) = name_node else {
        return;
    };
    builder.push_declaration(DeclarationDraft::new(
        DeclarationKind::Package,
        builder.text(name_node).to_string(),
        builder.range(name_node),
        builder.range(node),
    ));
}

fn handle_import(builder: &mut FactBuilder<'_>, node: Node) {
    let mut cursor = node.walk();
    let mut items = Vec::new();
    let mut grouped = false;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_spec" => items.push(import_spec_item(builder, child)),
            "import_spec_list" => {
                grouped = true;
                let mut inner = child.walk();
                for spec in child.children(&mut inner) {
                    if spec.kind() == "import_spec" {
                        items.push(import_spec_item(builder, spec));
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
    builder.push_import(form, None, None, None, items, builder.range(node));
}

fn import_spec_item(builder: &FactBuilder<'_>, spec: Node) -> crate::model::ImportItem {
    let name_node = spec.child_by_field_name("name");
    let path_node = spec.child_by_field_name("path");
    let target = path_node
        .map(|path| unquote(builder.text(path)).to_string())
        .unwrap_or_default();
    let target_range = path_node
        .map(|path| builder.range(path))
        .unwrap_or_else(|| builder.range(spec));
    let (alias, category) = match name_node {
        Some(name) if name.kind() == "blank_identifier" => (
            Some(("_".to_string(), builder.range(name))),
            ImportCategory::Blank,
        ),
        Some(name) if name.kind() == "dot" => (
            Some((".".to_string(), builder.range(name))),
            ImportCategory::Dot,
        ),
        Some(name) => (
            Some((builder.text(name).to_string(), builder.range(name))),
            ImportCategory::Normal,
        ),
        None => (None, ImportCategory::Normal),
    };
    import_item(
        target,
        target_range,
        alias,
        false,
        category,
        builder.range(spec),
    )
}

fn unquote(text: &str) -> &str {
    let trimmed = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('`')
                .and_then(|rest| rest.strip_suffix('`'))
        });
    trimmed.unwrap_or(text)
}

fn handle_type_declaration(builder: &mut FactBuilder<'_>, node: Node) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_spec" => handle_type_spec(builder, child),
            "type_alias" => handle_type_alias(builder, child),
            _ => {}
        }
    }
}

fn handle_type_spec(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let type_node = node.child_by_field_name("type");
    let name = builder.text(name_node).to_string();
    let range = builder.range(node);
    let kind = match type_node.map(|node| node.kind()) {
        Some("struct_type") => DeclarationKind::Struct,
        Some("interface_type") => DeclarationKind::Interface,
        // `type X Y` declares a distinct nominal type; it is not an alias.
        _ => DeclarationKind::NamedType,
    };
    let type_node =
        type_node.filter(|node| matches!(node.kind(), "struct_type" | "interface_type"));
    let mut draft = DeclarationDraft::new(kind, name.clone(), builder.range(name_node), range);
    if let Some(type_node) = type_node {
        draft = draft.with_body(Some(builder.range(type_node)));
    }
    builder.push_declaration(draft);

    let Some(type_node) = type_node else {
        return;
    };
    let header = header_range(builder, node, type_node);
    match type_node.kind() {
        "struct_type" => {
            builder.push_scope(ScopeKind::Struct, Some(name), range, header);
            emit_struct_fields(builder, type_node);
            builder.pop_scope();
        }
        "interface_type" => {
            builder.push_scope(ScopeKind::Interface, Some(name), range, header);
            emit_interface_members(builder, type_node);
            builder.pop_scope();
        }
        _ => {}
    }
}

fn header_range(builder: &FactBuilder<'_>, node: Node, body: Node) -> Option<SourceRange> {
    let start = node.start_byte() as u32;
    let end = body.start_byte() as u32;
    if end <= start {
        return None;
    }
    Some(builder.range_from_offsets(start, end))
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

fn emit_struct_fields(builder: &mut FactBuilder<'_>, struct_type: Node) {
    let mut cursor = struct_type.walk();
    for child in struct_type.children(&mut cursor) {
        if child.kind() != "field_declaration_list" {
            continue;
        }
        let mut inner = child.walk();
        for field in child.children(&mut inner) {
            if field.kind() != "field_declaration" {
                continue;
            }
            let type_node = field.child_by_field_name("type");
            match field.child_by_field_name("name") {
                Some(name_node) => {
                    builder.push_declaration(DeclarationDraft::new(
                        DeclarationKind::Field,
                        builder.text(name_node).to_string(),
                        builder.range(name_node),
                        builder.range(field),
                    ));
                }
                // Embedded field: the field has no written name. The type is
                // recorded as an unresolved embedded-type reference.
                None => {
                    if let Some(type_node) = type_node {
                        builder.push_reference(
                            ReferenceKind::EmbeddedType,
                            builder.text(type_node),
                            builder.range(type_node),
                            None,
                        );
                    }
                }
            }
        }
    }
}

fn emit_interface_members(builder: &mut FactBuilder<'_>, interface_type: Node) {
    let mut cursor = interface_type.walk();
    for child in interface_type.children(&mut cursor) {
        match child.kind() {
            "method_elem" => {
                let Some(name_node) = child.child_by_field_name("name") else {
                    continue;
                };
                builder.push_declaration(DeclarationDraft::new(
                    DeclarationKind::Method,
                    builder.text(name_node).to_string(),
                    builder.range(name_node),
                    builder.range(child),
                ));
            }
            // Embedded interface: recorded as an unresolved reference.
            "type_elem" => {
                let text = builder.text(child).trim().to_string();
                builder.push_reference(
                    ReferenceKind::EmbeddedType,
                    text,
                    builder.range(child),
                    None,
                );
            }
            _ => {}
        }
    }
}

fn handle_value_declaration(builder: &mut FactBuilder<'_>, node: Node, kind: DeclarationKind) {
    // Function-local `var`/`const` declarations are deliberately not extracted
    // in TASK 1, matching the Python policy for function-local bindings.
    if builder.current_scope_kind() != ScopeKind::File {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let specs: Vec<Node> = match child.kind() {
            "const_spec" | "var_spec" => vec![child],
            "const_spec_list" | "var_spec_list" => {
                let mut inner = child.walk();
                child
                    .children(&mut inner)
                    .filter(|spec| matches!(spec.kind(), "const_spec" | "var_spec"))
                    .collect()
            }
            _ => Vec::new(),
        };
        for spec in specs {
            emit_value_spec(builder, spec, kind);
        }
    }
}

fn emit_value_spec(builder: &mut FactBuilder<'_>, spec: Node, kind: DeclarationKind) {
    let spec_range = builder.range(spec);
    let mut cursor = spec.walk();
    let mut names: Vec<Node> = Vec::new();
    for (index, child) in spec.children(&mut cursor).enumerate() {
        if !child.is_named() || !matches!(child.kind(), "identifier" | "field_identifier") {
            continue;
        }
        // `const A = B` also contains an `identifier`, but it lives inside the
        // value `expression_list` and carries no `name` field.
        if spec.field_name_for_child(index as u32) != Some("name") {
            continue;
        }
        names.push(child);
    }
    if names.is_empty() {
        return;
    }
    for name_node in names {
        builder.push_declaration(DeclarationDraft::new(
            kind,
            builder.text(name_node).to_string(),
            builder.range(name_node),
            spec_range,
        ));
    }
}

fn handle_function(builder: &mut FactBuilder<'_>, node: Node) {
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
    emit_go_test_evidence(builder, node, name_node, &name, declaration_id);
    if let Some(body) = body {
        builder.push_scope(ScopeKind::Function, Some(name), range, None);
        visit_children(builder, body);
        builder.pop_scope();
    }
}

/// A Go method is declared at file scope. The receiver type is *not* a lexical
/// container, so it is recorded as an unresolved reference on the method.
fn handle_method(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = builder.text(name_node).to_string();
    let range = builder.range(node);
    let body = node.child_by_field_name("body");
    let declaration_id = builder.push_declaration(
        DeclarationDraft::new(
            DeclarationKind::Method,
            name.clone(),
            builder.range(name_node),
            range,
        )
        .with_body(body.map(|body| builder.range(body)))
        .with_flag(DeclarationFlag::Receiver),
    );
    if let Some(receiver) = node.child_by_field_name("receiver") {
        emit_receiver(builder, receiver, declaration_id);
    }
    emit_go_test_evidence(builder, node, name_node, &name, declaration_id);
    if let Some(body) = body {
        builder.push_scope(ScopeKind::Method, Some(name), range, None);
        visit_children(builder, body);
        builder.pop_scope();
    }
}

fn emit_receiver(builder: &mut FactBuilder<'_>, receiver: Node, declaration_id: u32) {
    let mut cursor = receiver.walk();
    for child in receiver.children(&mut cursor) {
        if child.kind() != "parameter_declaration" {
            continue;
        }
        if let Some(type_node) = child.child_by_field_name("type") {
            builder.push_reference(
                ReferenceKind::ReceiverType,
                builder.text(type_node),
                builder.range(type_node),
                Some(declaration_id),
            );
        }
    }
}

fn handle_func_literal(builder: &mut FactBuilder<'_>, node: Node) {
    let range = builder.range(node);
    builder.push_scope(ScopeKind::Closure, None::<String>, range, None);
    visit_children(builder, node);
    builder.pop_scope();
}

/// Go test evidence is purely syntactic:
///
/// * the written name starts with `Test`/`Benchmark`/`Fuzz` followed by a
///   non-lowercase character (the rule Go's own tooling documents);
/// * the first parameter is `*testing.T`/`*testing.B`/`*testing.F`.
///
/// It is evidence, not a claim that `go test` would collect the function.
fn emit_go_test_evidence(
    builder: &mut FactBuilder<'_>,
    node: Node,
    name_node: Node,
    name: &str,
    declaration_id: u32,
) {
    for prefix in ["Test", "Benchmark", "Fuzz"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            let valid = rest
                .chars()
                .next()
                .map(|first| !first.is_ascii_lowercase())
                .unwrap_or(false);
            if valid {
                builder.add_test_evidence(
                    declaration_id,
                    TestEvidence {
                        kind: TestEvidenceKind::FunctionNameConvention,
                        detail: format!("{prefix} prefix"),
                        range: builder.range(name_node),
                    },
                );
            }
            break;
        }
    }
    if let Some(parameters) = node.child_by_field_name("parameters") {
        if let Some(detail) = testing_signature(builder, parameters) {
            builder.add_test_evidence(
                declaration_id,
                TestEvidence {
                    kind: TestEvidenceKind::SignatureShape,
                    detail,
                    range: builder.range(parameters),
                },
            );
        }
    }
}

/// True when the parameter list is a single `*testing.X` parameter.
fn testing_signature(builder: &FactBuilder<'_>, parameters: Node) -> Option<String> {
    let mut cursor = parameters.walk();
    let mut found: Option<Node> = None;
    for child in parameters.children(&mut cursor) {
        if child.kind() != "parameter_declaration" {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(child);
    }
    let parameter = found?;
    let type_node = parameter.child_by_field_name("type")?;
    let written = builder.text(type_node);
    let receiver = written.strip_prefix("*testing.")?;
    if matches!(receiver, "T" | "B" | "F") {
        Some(format!("single *testing.{receiver} parameter"))
    } else {
        None
    }
}

fn emit_call(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    let form = match function.kind() {
        "identifier" => CallLikeForm::PlainName,
        "selector_expression" => CallLikeForm::MemberSelector,
        // `fns[i](x)` and `Generic[T](x)` are both `index_expression`; the
        // grammar does not distinguish indexing from generic instantiation.
        _ => CallLikeForm::Indirect,
    };
    let type_arguments = node.child_by_field_name("type_arguments").map(|arguments| {
        (
            builder.text(arguments).to_string(),
            builder.range(arguments),
        )
    });
    builder.push_call(
        form,
        builder.text(function),
        builder.range(function),
        builder.range(node),
        type_arguments,
        false,
        false,
    );
}
