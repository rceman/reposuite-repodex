//! Go language adapter.
//!
//! Boundaries:
//!
//! * a method is declared at file scope with a receiver; it is never nested
//!   inside the receiver type. Receiver syntax is recorded separately;
//! * build tags are never evaluated;
//! * selectors are never resolved and promoted methods are never inferred;
//! * `t.Run` is never treated as proof of test ownership;
//! * `T(x)` is never classified as a call *or* as a conversion. Tree-sitter's
//!   Go grammar splits the shape by argument count: `T(x)` and `Generic[int](x)`
//!   with a single argument become `type_conversion_expression`, while two or
//!   more arguments become `call_expression`. Because `Generic[int](x)` is
//!   simultaneously a valid generic invocation and a valid conversion, both
//!   shapes are reported as `CallLikeOccurrence`s and neither is resolved.
//!   `T(x)` keeps the grammar's own `type_conversion` form label.

use tree_sitter::{Language, Node, Tree};

use std::collections::HashSet;

use crate::model::{
    BindingKind, CallLikeForm, DeclarationFlag, DeclarationKind, ImportCategory, ImportForm,
    LanguageId, ReferenceKind, ScopeKind, SourceRange, TestEvidence, TestEvidenceKind,
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
        "type_declaration" => {
            handle_type_declaration(builder, node);
            // Descend into spec values so nested `func` literals (and the calls
            // inside them) are reachable everywhere, not only in a body.
            visit_children(builder, node);
        }
        "const_declaration" => {
            handle_value_declaration(builder, node, DeclarationKind::Constant);
            visit_children(builder, node);
        }
        "var_declaration" => {
            handle_value_declaration(builder, node, DeclarationKind::Variable);
            visit_children(builder, node);
        }
        "function_declaration" => handle_function(builder, node),
        "method_declaration" => handle_method(builder, node),
        "func_literal" => handle_func_literal(builder, node),
        "call_expression" => {
            emit_call(builder, node);
            visit_children(builder, node);
        }
        // `T(x)` and `Generic[int](x)` with a single argument. The grammar calls
        // this a conversion, but the identical syntax is a generic invocation.
        // Dropping it would lose a real possible call, so it is recorded with
        // the grammar's own form label and never resolved either way.
        "type_conversion_expression" => {
            emit_type_conversion(builder, node);
            visit_children(builder, node);
        }
        _ => visit_children(builder, node),
    }
}

/// Record a `type_conversion_expression` as an unresolved call-shaped
/// occurrence.
///
/// The callee is a type expression (`int64`, `Generic[int]`, `pkg.Generic[int]`,
/// `obj.Generic[int]`). RepoDex reports the written form, the type arguments
/// when the syntax has them, and the `type_conversion` form label. It does not
/// claim the expression is a conversion and does not claim it is a call.
fn emit_type_conversion(builder: &mut FactBuilder<'_>, node: Node) {
    let mut cursor = node.walk();
    // The callee is the type expression before the argument list. The grammar
    // does not give it a field name on this node.
    let callee = node
        .children(&mut cursor)
        .find(|child| child.is_named() && child.kind() != "argument_list");
    let Some(callee) = callee else {
        return;
    };
    let type_arguments = type_arguments_of(builder, callee);
    builder.push_call(
        CallLikeForm::TypeConversion,
        builder.text(callee),
        builder.range(callee),
        builder.range(node),
        type_arguments,
        false,
        false,
    );
}

/// Written type arguments of a type expression, when the syntax carries them.
///
/// `generic_type` (`Generic[int]`, `pkg.Generic[int]`) carries a
/// `type_arguments` field. A plain `type_identifier` (`int64`) carries none.
fn type_arguments_of(
    builder: &FactBuilder<'_>,
    callee: Node,
) -> Option<(String, crate::model::SourceRange)> {
    callee
        .child_by_field_name("type_arguments")
        .map(|arguments| {
            (
                builder.text(arguments).to_string(),
                builder.range(arguments),
            )
        })
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
        emit_local_bindings(
            builder,
            node,
            body,
            BindingKind::FunctionParameter,
            BindingKind::FunctionResult,
            None,
        );
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
        emit_local_bindings(
            builder,
            node,
            body,
            BindingKind::FunctionParameter,
            BindingKind::FunctionResult,
            node.child_by_field_name("receiver"),
        );
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
    if let Some(body) = node.child_by_field_name("body") {
        emit_local_bindings(
            builder,
            node,
            body,
            BindingKind::FunctionLiteralParameter,
            BindingKind::FunctionLiteralResult,
            None,
        );
    }
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

// ---------------------------------------------------------------------------
// TASK 4B — function-local lexical bindings (LocalBindingOccurrence)
//
// Persisted `:=`/`var`/`const`/`type`/parameter/receiver/range/type-switch/
// select-receive names with bounded `visibility_ranges`, so a later candidate
// layer can tell when a closer local name blocks a package-level function —
// without reparsing. `_` is never a binding; labels and field/selector names
// are never bindings. Visibility is the existing `SourceRange` contract
// (half-open bytes, 0-based rows, UTF-8 byte columns).
// ---------------------------------------------------------------------------

/// The innermost lexical block while walking a body: `end` bounds block-scoped
/// binding visibility; `declared` holds the names introduced directly in this
/// block so `:=` can distinguish a new binding from a same-block redeclaration.
struct LexBlock {
    end: u32,
    declared: HashSet<String>,
}

impl LexBlock {
    fn new(end: u32) -> Self {
        LexBlock {
            end,
            declared: HashSet::new(),
        }
    }
}

/// Emit signature bindings (receiver, parameters, named results) for a
/// `function_declaration`/`method_declaration`/`func_literal`, then walk the
/// body block for statement-level bindings. Signature names seed the body's
/// declared set so `:=` redeclaration treats them as same-block (§17/§18).
fn emit_local_bindings(
    builder: &mut FactBuilder<'_>,
    sig_node: Node,
    body: Node,
    param_kind: BindingKind,
    result_kind: BindingKind,
    receiver: Option<Node>,
) {
    let mut declared = HashSet::new();
    let body_range = builder.range(body);
    // Generic type parameters are blockers too: `T(v)` is a `plain_name` call,
    // and `T` is in scope across the whole declaration (signature + body).
    if let Some(type_params) = sig_node.child_by_field_name("type_parameters") {
        let sig_range = vec![builder.range(sig_node)];
        let mut cursor = type_params.walk();
        for decl in type_params.children(&mut cursor) {
            if decl.kind() != "type_parameter_declaration" {
                continue;
            }
            if let Some(name_node) = decl.child_by_field_name("name") {
                if name_node.kind() == "identifier" {
                    let name = builder.text(name_node).to_string();
                    if name != "_" {
                        builder.push_binding(
                            BindingKind::TypeParameter,
                            name.clone(),
                            builder.range(name_node),
                            builder.range(decl),
                            sig_range.clone(),
                            false,
                        );
                        declared.insert(name);
                    }
                }
            }
        }
    }
    if let Some(recv) = receiver {
        emit_param_list_bindings(
            builder,
            recv,
            body_range,
            BindingKind::MethodReceiver,
            &mut declared,
        );
    }
    if let Some(params) = sig_node.child_by_field_name("parameters") {
        emit_param_list_bindings(builder, params, body_range, param_kind, &mut declared);
    }
    // `result` is a parameter_list only when it carries (possibly named)
    // result variables; a bare `int` result is a `_simple_type` with no names.
    if let Some(result) = sig_node.child_by_field_name("result") {
        if result.kind() == "parameter_list" {
            emit_param_list_bindings(builder, result, body_range, result_kind, &mut declared);
        }
    }
    let mut block = LexBlock {
        end: body.end_byte() as u32,
        declared,
    };
    visit_block_statements(builder, body, &mut block);
}

/// Emit a binding for each written `name` identifier in a `parameter_list`,
/// `receiver`, or named-result list. Skips `_`.
fn emit_param_list_bindings(
    builder: &mut FactBuilder<'_>,
    list: Node,
    body_range: SourceRange,
    kind: BindingKind,
    declared: &mut HashSet<String>,
) {
    let mut cursor = list.walk();
    for param in list.children(&mut cursor) {
        if !matches!(
            param.kind(),
            "parameter_declaration" | "variadic_parameter_declaration"
        ) {
            continue;
        }
        let mut inner = param.walk();
        for (index, child) in param.children(&mut inner).enumerate() {
            if param.field_name_for_child(index as u32) != Some("name")
                || child.kind() != "identifier"
            {
                continue;
            }
            let name = builder.text(child).to_string();
            if name == "_" {
                continue;
            }
            builder.push_binding(
                kind,
                name.clone(),
                builder.range(child),
                builder.range(param),
                vec![body_range],
                false,
            );
            declared.insert(name);
        }
    }
}

/// Iterate the statements of a `block`'s `statement_list`.
fn visit_block_statements(builder: &mut FactBuilder<'_>, block_node: Node, block: &mut LexBlock) {
    let mut cursor = block_node.walk();
    for child in block_node.children(&mut cursor) {
        if child.kind() == "statement_list" {
            let mut inner = child.walk();
            for stmt in child.children(&mut inner) {
                if stmt.is_named() {
                    visit_statement(builder, stmt, block);
                }
            }
        }
    }
}

/// The identifier nodes of an `expression_list` (LHS of `:=`, range, receive).
fn expr_list_identifiers(list: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = list.walk();
    list.children(&mut cursor)
        .filter(|c| c.is_named() && c.kind() == "identifier")
        .collect()
}

/// True when `node` has a direct `:=` child token (a declaration, not `=`).
fn has_colon_eq(node: Node) -> bool {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).any(|c| c.kind() == ":=");
    found
}

fn visit_statement(builder: &mut FactBuilder<'_>, node: Node, block: &mut LexBlock) {
    match node.kind() {
        // A bare `{ ... }` is a new lexical block.
        "block" => {
            let mut inner = LexBlock::new(node.end_byte() as u32);
            visit_block_statements(builder, node, &mut inner);
        }
        "short_var_declaration" => {
            emit_short_var(builder, node, block);
            // A `func` literal in the RHS still introduces its own bindings.
            if let Some(right) = node.child_by_field_name("right") {
                visit_statement(builder, right, block);
            }
        }
        "var_declaration" | "const_declaration" | "type_declaration" => {
            emit_local_decl(builder, node, block);
        }
        "if_statement" => emit_if(builder, node, block),
        "for_statement" => emit_for(builder, node, block),
        "expression_switch_statement" => emit_switch(builder, node, block),
        "type_switch_statement" => emit_type_switch(builder, node, block),
        "select_statement" => emit_select(builder, node, block),
        // A `func` literal manages its own signature/body bindings through
        // `handle_func_literal`; do not re-descend (it would double-emit and
        // wrongly share the outer block's declared set).
        "func_literal" => {}
        // A label is a separate namespace — never a value binding (§33).
        "labeled_statement" => visit_children_bindings(builder, node, block),
        _ => visit_children_bindings(builder, node, block),
    }
}

/// Recurse into a node's named children keeping the same block — this finds
/// nested `func` literals and block constructs inside expressions/statements.
fn visit_children_bindings(builder: &mut FactBuilder<'_>, node: Node, block: &mut LexBlock) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            visit_statement(builder, child, block);
        }
    }
}

/// `x, y := ...`: emit a `ShortVariable` binding for each LHS identifier that
/// is newly introduced in this block. A name already declared in *this* block
/// (or seeded from the signature for the body block) is a redeclaration and
/// produces no new binding. Visibility runs from the end of the declaration to
/// the end of the block, so the RHS is never covered (§16).
fn emit_short_var(builder: &mut FactBuilder<'_>, node: Node, block: &mut LexBlock) {
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    let visibility = vec![builder.range_from_offsets(node.end_byte() as u32, block.end)];
    for ident in expr_list_identifiers(left) {
        let name = builder.text(ident).to_string();
        if name == "_" || block.declared.contains(&name) {
            continue;
        }
        builder.push_binding(
            BindingKind::ShortVariable,
            name.clone(),
            builder.range(ident),
            builder.range(node),
            visibility.clone(),
            false,
        );
        block.declared.insert(name);
    }
}

/// Function-local `var`/`const`/`type` declarations. Each spec's scope begins
/// at the end of that spec and ends at the block end (§12/§13). `_` skipped.
fn emit_local_decl(builder: &mut FactBuilder<'_>, node: Node, block: &mut LexBlock) {
    let kind = match node.kind() {
        "const_declaration" => BindingKind::Constant,
        "type_declaration" => BindingKind::LocalType,
        _ => BindingKind::Variable,
    };
    let mut specs = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "var_spec" | "const_spec" | "type_spec" | "type_alias" => specs.push(child),
            "var_spec_list" | "const_spec_list" | "type_spec_list" => {
                let mut inner = child.walk();
                for spec in child.children(&mut inner) {
                    if matches!(
                        spec.kind(),
                        "var_spec" | "const_spec" | "type_spec" | "type_alias"
                    ) {
                        specs.push(spec);
                    }
                }
            }
            _ => {}
        }
    }
    for spec in specs {
        let spec_end = spec.end_byte() as u32;
        let visibility = vec![builder.range_from_offsets(spec_end, block.end)];
        let mut inner = spec.walk();
        for (index, child) in spec.children(&mut inner).enumerate() {
            if !child.is_named()
                || spec.field_name_for_child(index as u32) != Some("name")
                || !matches!(
                    child.kind(),
                    "identifier" | "field_identifier" | "type_identifier"
                )
            {
                continue;
            }
            let name = builder.text(child).to_string();
            if name == "_" {
                continue;
            }
            builder.push_binding(
                kind,
                name.clone(),
                builder.range(child),
                builder.range(spec),
                visibility.clone(),
                false,
            );
            block.declared.insert(name);
        }
        // A `func` literal in the spec's value introduces its own bindings.
        if let Some(value) = spec.child_by_field_name("value") {
            visit_statement(builder, value, block);
        }
    }
}

/// Emit bindings for a simple `initializer`/`:=` statement (if/for/switch init)
/// with the given visibility. These names live in the statement's implicit
/// block; a `:=` inside a branch shadows them (outer scope), so no shared
/// declared-set is needed.
fn emit_initializer(
    builder: &mut FactBuilder<'_>,
    init: Option<Node>,
    visibility: Vec<SourceRange>,
) {
    let Some(init) = init else { return };
    if init.kind() == "short_var_declaration" {
        if let Some(left) = init.child_by_field_name("left") {
            for ident in expr_list_identifiers(left) {
                let name = builder.text(ident).to_string();
                if name == "_" {
                    continue;
                }
                builder.push_binding(
                    BindingKind::ShortVariable,
                    name,
                    builder.range(ident),
                    builder.range(init),
                    visibility.clone(),
                    false,
                );
            }
        }
    }
}

/// `if init; cond { A } else { B }` — init names cover the whole `if` (§21).
fn emit_if(builder: &mut FactBuilder<'_>, node: Node, block: &mut LexBlock) {
    let visibility = vec![builder.range(node)];
    emit_initializer(builder, node.child_by_field_name("initializer"), visibility);
    if let Some(init) = node.child_by_field_name("initializer") {
        if let Some(right) = init.child_by_field_name("right") {
            visit_statement(builder, right, block);
        }
    }
    if let Some(cond) = node.child_by_field_name("condition") {
        visit_statement(builder, cond, block);
    }
    if let Some(cons) = node.child_by_field_name("consequence") {
        let mut inner = LexBlock::new(cons.end_byte() as u32);
        visit_block_statements(builder, cons, &mut inner);
    }
    if let Some(alt) = node.child_by_field_name("alternative") {
        visit_statement(builder, alt, block);
    }
}

/// `for` — clause init, range vars, or plain; each body is a new block (§23/§24).
fn emit_for(builder: &mut FactBuilder<'_>, node: Node, block: &mut LexBlock) {
    let body = node.child_by_field_name("body");
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "for_clause" => {
                emit_initializer(
                    builder,
                    child.child_by_field_name("initializer"),
                    vec![builder.range(node)],
                );
                if let Some(init) = child.child_by_field_name("initializer") {
                    if let Some(right) = init.child_by_field_name("right") {
                        visit_statement(builder, right, block);
                    }
                }
                for field in ["condition", "update"] {
                    if let Some(n) = child.child_by_field_name(field) {
                        visit_statement(builder, n, block);
                    }
                }
            }
            "range_clause" => {
                // `for k, v := range x` — `:=` declares; `=` does not (§25).
                if has_colon_eq(child) {
                    if let (Some(left), Some(body)) = (child.child_by_field_name("left"), body) {
                        let body_range = vec![builder.range(body)];
                        for ident in expr_list_identifiers(left) {
                            let name = builder.text(ident).to_string();
                            if name == "_" {
                                continue;
                            }
                            builder.push_binding(
                                BindingKind::RangeVariable,
                                name,
                                builder.range(ident),
                                builder.range(child),
                                body_range.clone(),
                                false,
                            );
                        }
                    }
                }
                if let Some(right) = child.child_by_field_name("right") {
                    visit_statement(builder, right, block);
                }
            }
            _ => {
                if child.is_named() && child.kind() != "block" {
                    visit_statement(builder, child, block);
                }
            }
        }
    }
    if let Some(body) = body {
        let mut inner = LexBlock::new(body.end_byte() as u32);
        visit_block_statements(builder, body, &mut inner);
    }
}

/// `switch init; v { case ... }` — init names cover the whole switch; each
/// `case`/`default` body is its own implicit block (§22/§30).
fn emit_switch(builder: &mut FactBuilder<'_>, node: Node, block: &mut LexBlock) {
    emit_initializer(
        builder,
        node.child_by_field_name("initializer"),
        vec![builder.range(node)],
    );
    if let Some(init) = node.child_by_field_name("initializer") {
        if let Some(right) = init.child_by_field_name("right") {
            visit_statement(builder, right, block);
        }
    }
    if let Some(value) = node.child_by_field_name("value") {
        visit_statement(builder, value, block);
    }
    emit_clause_bodies(builder, node, block);
}

/// Iterate `expression_case`/`type_case`/`default_case` bodies, each a fresh
/// block so a `:=` in one clause cannot leak into a sibling (§30).
fn emit_clause_bodies(builder: &mut FactBuilder<'_>, switch_node: Node, block: &mut LexBlock) {
    let mut cursor = switch_node.walk();
    for case in switch_node.children(&mut cursor) {
        if !matches!(
            case.kind(),
            "expression_case" | "type_case" | "default_case" | "communication_case"
        ) {
            continue;
        }
        let mut inner_cursor = case.walk();
        for part in case.children(&mut inner_cursor) {
            if part.kind() == "statement_list" {
                let mut clause = LexBlock::new(part.end_byte() as u32);
                let mut inner = part.walk();
                for stmt in part.children(&mut inner) {
                    if stmt.is_named() {
                        visit_statement(builder, stmt, &mut clause);
                    }
                }
            }
        }
        // A clause's guard expressions (case values) are not bindings; descend
        // into them for nested func literals.
        let _ = block;
    }
}

/// `switch v := x.(type) { case T: ... }` — the guard variable is one source
/// occurrence visible in *every* clause's implicit block (§27/§28): disjoint
/// `visibility_ranges`, not a per-clause duplicate.
fn emit_type_switch(builder: &mut FactBuilder<'_>, node: Node, block: &mut LexBlock) {
    emit_initializer(
        builder,
        node.child_by_field_name("initializer"),
        vec![builder.range(node)],
    );
    // Collect each clause's statement_list range as the variable's visibility.
    let mut clause_ranges = Vec::new();
    let mut cursor = node.walk();
    for case in node.children(&mut cursor) {
        if !matches!(case.kind(), "type_case" | "default_case") {
            continue;
        }
        let mut inner = case.walk();
        for part in case.children(&mut inner) {
            if part.kind() == "statement_list" {
                clause_ranges.push(builder.range(part));
            }
        }
    }
    // `alias` is the `v :=` guard (an expression_list before `:=`).
    if has_colon_eq(node) {
        if let Some(alias) = node.child_by_field_name("alias") {
            for ident in expr_list_identifiers(alias) {
                let name = builder.text(ident).to_string();
                if name == "_" {
                    continue;
                }
                builder.push_binding(
                    BindingKind::TypeSwitchVariable,
                    name,
                    builder.range(ident),
                    builder.range(node),
                    clause_ranges.clone(),
                    false,
                );
            }
        }
    }
    emit_clause_bodies(builder, node, block);
}

/// `select { case v := <-ch: ... }` — a `:=` receive declares in that clause's
/// implicit block only; `=` does not (§29).
fn emit_select(builder: &mut FactBuilder<'_>, node: Node, block: &mut LexBlock) {
    let mut cursor = node.walk();
    for case in node.children(&mut cursor) {
        if !matches!(case.kind(), "communication_case" | "default_case") {
            continue;
        }
        // Find this clause's statement_list for both recursion and visibility.
        let mut stmt_list = None;
        let mut comm = None;
        let mut inner = case.walk();
        for part in case.children(&mut inner) {
            match part.kind() {
                "statement_list" => stmt_list = Some(part),
                "receive_statement" | "send_statement" => comm = Some(part),
                _ => {}
            }
        }
        if let Some(recv) = comm {
            if recv.kind() == "receive_statement" && has_colon_eq(recv) {
                if let (Some(left), Some(body)) = (recv.child_by_field_name("left"), stmt_list) {
                    let range = vec![builder.range(body)];
                    for ident in expr_list_identifiers(left) {
                        let name = builder.text(ident).to_string();
                        if name == "_" {
                            continue;
                        }
                        builder.push_binding(
                            BindingKind::SelectReceiveVariable,
                            name,
                            builder.range(ident),
                            builder.range(recv),
                            range.clone(),
                            false,
                        );
                    }
                }
            }
            visit_statement(builder, recv, block);
        }
        if let Some(list) = stmt_list {
            let mut clause = LexBlock::new(list.end_byte() as u32);
            let mut sc = list.walk();
            for stmt in list.children(&mut sc) {
                if stmt.is_named() {
                    visit_statement(builder, stmt, &mut clause);
                }
            }
        }
    }
}
