//! Python language adapter.
//!
//! Boundaries:
//!
//! * decorators, inheritance, dynamic imports and monkey patches are never
//!   resolved;
//! * uppercase naming alone never proves that a binding is a constant, so a
//!   plain module-level assignment is a `Variable`. Only an explicit
//!   `Final`/`Final[...]` annotation produces a `Constant`;
//! * function-local bindings are not extracted in TASK 1;
//! * `Thing()` is only ever a call-shaped expression, never a constructor claim.

use tree_sitter::{Language, Node, Tree};

use crate::model::{
    CallLikeForm, DeclarationFlag, DeclarationKind, ImportCategory, ImportForm, LanguageId,
    ReferenceKind, ScopeKind, SourceRange, TestEvidence, TestEvidenceKind,
};

use super::builder::{import_item, DeclarationDraft, FactBuilder};
use super::recovery::RecoveryScanner;
use super::{AdapterError, GrammarInfo, LanguageAdapter};

pub struct PythonAdapter {
    recovery: RecoveryScanner,
}

impl PythonAdapter {
    pub fn new() -> Result<Self, AdapterError> {
        let language: Language = tree_sitter_python::LANGUAGE.into();
        let recovery = RecoveryScanner::new(LanguageId::Python, &language).map_err(|message| {
            AdapterError {
                language: Some(LanguageId::Python),
                message,
            }
        })?;
        Ok(Self { recovery })
    }
}

const GRAMMAR: GrammarInfo = GrammarInfo {
    crate_name: "tree-sitter-python",
    crate_version: "0.25.0",
    upstream_repository: "https://github.com/tree-sitter/tree-sitter-python",
    license: "MIT",
    upstream_queries: &["queries/tags.scm", "queries/highlights.scm"],
    repo_queries: &["queries/python/recovery.scm"],
    root_node_kind: "module",
    tree_sitter_compatibility: "ABI 15 (tree-sitter 0.27.x)",
};

impl LanguageAdapter for PythonAdapter {
    fn language(&self) -> LanguageId {
        LanguageId::Python
    }

    fn grammar(&self) -> GrammarInfo {
        GRAMMAR
    }

    fn ts_language(&self) -> Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn recovery(&self) -> &RecoveryScanner {
        &self.recovery
    }

    fn extract(&self, builder: &mut FactBuilder<'_>, tree: &Tree) {
        emit_file_evidence(builder);
        visit_children(builder, tree.root_node());
    }
}

/// `test_*.py` / `*_test.py` are file naming conventions only.
fn emit_file_evidence(builder: &mut FactBuilder<'_>) {
    let path = builder.file().relative_path.clone();
    let file_name = path.rsplit('/').next().unwrap_or(&path);
    let detail = if file_name.starts_with("test_") && file_name.ends_with(".py") {
        Some("test_*.py prefix")
    } else if file_name.ends_with("_test.py") {
        Some("*_test.py suffix")
    } else {
        None
    };
    if let Some(detail) = detail {
        builder.add_file_test_evidence(TestEvidence {
            kind: TestEvidenceKind::FileNameConvention,
            detail: detail.to_string(),
            range: builder.range_from_offsets(0, 0),
        });
    }
}

fn visit(builder: &mut FactBuilder<'_>, node: Node) {
    match node.kind() {
        "decorated_definition" => handle_decorated(builder, node),
        "function_definition" => handle_function(builder, node, &[], None),
        "class_definition" => handle_class(builder, node, &[], None),
        "import_statement" => handle_import(builder, node),
        "import_from_statement" => handle_import_from(builder, node),
        "assignment" => handle_assignment(builder, node),
        "lambda" => handle_lambda(builder, node),
        "call" => {
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

fn handle_decorated(builder: &mut FactBuilder<'_>, node: Node) {
    let mut decorators = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "decorator" {
            decorators.push(child);
        }
    }
    let Some(definition) = node.child_by_field_name("definition") else {
        return;
    };
    let full_range = builder.range(node);

    // Decorator syntax is recorded before entering the decorated scope: a
    // decorator expression is evaluated in the enclosing scope.
    for decorator in &decorators {
        emit_decorator(builder, *decorator);
    }

    match definition.kind() {
        "function_definition" => {
            handle_function(builder, definition, &decorators, Some(full_range))
        }
        "class_definition" => handle_class(builder, definition, &decorators, Some(full_range)),
        _ => visit_children(builder, node),
    }
}

/// Python evaluates default-argument expressions and base-class expressions
/// when the definition statement runs, not when the function/class is called.
/// Their call-like occurrences are therefore attributed to the enclosing scope.
fn emit_definition_time_expressions(builder: &mut FactBuilder<'_>, definition: Node) {
    match definition.kind() {
        "function_definition" => {
            if let Some(parameters) = definition.child_by_field_name("parameters") {
                visit(builder, parameters);
            }
        }
        "class_definition" => {
            if let Some(superclasses) = definition.child_by_field_name("superclasses") {
                visit(builder, superclasses);
            }
        }
        _ => {}
    }
}

fn emit_decorator(builder: &mut FactBuilder<'_>, decorator: Node) {
    let mut cursor = decorator.walk();
    let expression = decorator
        .named_children(&mut cursor)
        .find(|child| child.kind() != "comment");
    let Some(expression) = expression else {
        return;
    };
    // For `@dec(arg)` the decorator name is the callee; the call itself is
    // recorded separately as a call-like occurrence.
    let name_node = if expression.kind() == "call" {
        expression
            .child_by_field_name("function")
            .unwrap_or(expression)
    } else {
        expression
    };
    builder.push_reference(
        ReferenceKind::Decorator,
        builder.text(name_node),
        builder.range(name_node),
        None,
    );
    visit(builder, expression);
}

fn handle_function(
    builder: &mut FactBuilder<'_>,
    node: Node,
    decorators: &[Node],
    full_range: Option<SourceRange>,
) {
    emit_definition_time_expressions(builder, node);
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = builder.text(name_node).to_string();
    let range = full_range.unwrap_or_else(|| builder.range(node));
    let body = node.child_by_field_name("body");
    let is_method = builder.current_scope_kind() == ScopeKind::Class;
    let kind = if is_method {
        DeclarationKind::Method
    } else {
        DeclarationKind::Function
    };

    let mut draft = DeclarationDraft::new(kind, name.clone(), builder.range(name_node), range)
        .with_body(body.map(|body| builder.range(body)));
    if is_async(node) {
        draft = draft.with_flag(DeclarationFlag::Async);
    }
    let declaration_id = builder.push_declaration(draft);

    if name.starts_with("test_") {
        builder.add_test_evidence(
            declaration_id,
            TestEvidence {
                kind: TestEvidenceKind::FunctionNameConvention,
                detail: "test_ prefix".to_string(),
                range: builder.range(name_node),
            },
        );
    }
    for decorator in decorators {
        if let Some(evidence) = pytest_mark_evidence(builder, *decorator) {
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
        visit_children(builder, body);
        builder.pop_scope();
    }
}

fn handle_class(
    builder: &mut FactBuilder<'_>,
    node: Node,
    decorators: &[Node],
    full_range: Option<SourceRange>,
) {
    emit_definition_time_expressions(builder, node);
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = builder.text(name_node).to_string();
    let range = full_range.unwrap_or_else(|| builder.range(node));
    let body = node.child_by_field_name("body");
    let declaration_id = builder.push_declaration(
        DeclarationDraft::new(
            DeclarationKind::Class,
            name.clone(),
            builder.range(name_node),
            range,
        )
        .with_body(body.map(|body| builder.range(body))),
    );

    let mut bases = Vec::new();
    if let Some(superclasses) = node.child_by_field_name("superclasses") {
        let mut cursor = superclasses.walk();
        for child in superclasses.children(&mut cursor) {
            if !child.is_named() || child.kind() == "keyword_argument" {
                continue;
            }
            bases.push(child);
        }
    }
    for base in &bases {
        let written = builder.text(*base);
        builder.push_reference(
            ReferenceKind::BaseClass,
            written,
            builder.range(*base),
            Some(declaration_id),
        );
        // `class X(unittest.TestCase)` is syntactic base-clause evidence.
        let last_segment = written.rsplit('.').next().unwrap_or(written);
        if last_segment == "TestCase" {
            builder.add_test_evidence(
                declaration_id,
                TestEvidence {
                    kind: TestEvidenceKind::ClassBaseSyntax,
                    detail: written.to_string(),
                    range: builder.range(*base),
                },
            );
        }
    }
    let _ = decorators;

    if let Some(body) = body {
        builder.push_scope(ScopeKind::Class, Some(name), range, None);
        visit_children(builder, body);
        builder.pop_scope();
    }
}

fn is_async(node: Node) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|child| !child.is_named() && child.kind() == "async");
    found
}

/// `@pytest.mark.*` is the only decorator shape treated as test evidence.
fn pytest_mark_evidence(builder: &FactBuilder<'_>, decorator: Node) -> Option<TestEvidence> {
    let mut cursor = decorator.walk();
    let expression = decorator
        .named_children(&mut cursor)
        .find(|child| child.kind() != "comment")?;
    let written = builder.text(expression);
    if written.starts_with("pytest.mark.") {
        Some(TestEvidence {
            kind: TestEvidenceKind::Decorator,
            detail: format!("@{written}"),
            range: builder.range(decorator),
        })
    } else {
        None
    }
}

fn handle_lambda(builder: &mut FactBuilder<'_>, node: Node) {
    let range = builder.range(node);
    builder.push_scope(ScopeKind::Lambda, None::<String>, range, None);
    visit_children(builder, node);
    builder.pop_scope();
}

fn handle_import(builder: &mut FactBuilder<'_>, node: Node) {
    let mut items = Vec::new();
    let mut cursor = node.walk();
    for (index, child) in node.children(&mut cursor).enumerate() {
        if node.field_name_for_child(index as u32) != Some("name") {
            continue;
        }
        items.push(import_target_item(builder, child));
    }
    let form = if items.len() == 1 {
        ImportForm::Single
    } else {
        ImportForm::Grouped
    };
    builder.push_import(form, None, None, None, items, builder.range(node));
}

fn handle_import_from(builder: &mut FactBuilder<'_>, node: Node) {
    let mut items = Vec::new();
    let mut module: Option<String> = None;
    let mut module_range: Option<SourceRange> = None;
    let mut relative_levels: Option<u32> = None;
    let mut saw_wildcard = false;

    let mut cursor = node.walk();
    for (index, child) in node.children(&mut cursor).enumerate() {
        match node.field_name_for_child(index as u32) {
            Some("module_name") => match child.kind() {
                "relative_import" => {
                    relative_levels = Some(relative_dots(builder, child));
                    let mut inner = child.walk();
                    let dotted = child
                        .named_children(&mut inner)
                        .find(|grandchild| grandchild.kind() == "dotted_name");
                    if let Some(dotted) = dotted {
                        module = Some(builder.text(dotted).to_string());
                        module_range = Some(builder.range(dotted));
                    }
                }
                _ => {
                    module = Some(builder.text(child).to_string());
                    module_range = Some(builder.range(child));
                }
            },
            Some("name") => items.push(import_target_item(builder, child)),
            _ => {
                if child.kind() == "wildcard_import" {
                    saw_wildcard = true;
                    items.push(import_item(
                        "*",
                        builder.range(child),
                        None,
                        true,
                        ImportCategory::Normal,
                        builder.range(child),
                    ));
                }
            }
        }
    }

    let form = if saw_wildcard {
        ImportForm::Wildcard
    } else if items.len() == 1 {
        ImportForm::Single
    } else {
        ImportForm::Grouped
    };
    builder.push_import(
        form,
        module,
        module_range,
        relative_levels,
        items,
        builder.range(node),
    );
}

fn relative_dots(builder: &FactBuilder<'_>, relative_import: Node) -> u32 {
    let mut cursor = relative_import.walk();
    let mut dots = 0;
    for child in relative_import.children(&mut cursor) {
        if child.kind() == "import_prefix" {
            dots += builder.text(child).matches('.').count() as u32;
        }
    }
    dots
}

fn import_target_item(builder: &FactBuilder<'_>, child: Node) -> crate::model::ImportItem {
    match child.kind() {
        "aliased_import" => {
            let name_node = child.child_by_field_name("name");
            let alias_node = child.child_by_field_name("alias");
            let target = name_node.map(|node| builder.text(node)).unwrap_or_default();
            let target_range = name_node
                .map(|node| builder.range(node))
                .unwrap_or_else(|| builder.range(child));
            import_item(
                target,
                target_range,
                alias_node.map(|node| (builder.text(node).to_string(), builder.range(node))),
                false,
                ImportCategory::Normal,
                builder.range(child),
            )
        }
        _ => import_item(
            builder.text(child),
            builder.range(child),
            None,
            false,
            ImportCategory::Normal,
            builder.range(child),
        ),
    }
}

/// Module-level and class-level assignments only. Function-local bindings are
/// deliberately not extracted in TASK 1.
///
/// The value side is visited in every case, including for function-local
/// bindings and for attribute/subscript targets that declare nothing: a call
/// inside an expression is still a call-shaped occurrence.
fn handle_assignment(builder: &mut FactBuilder<'_>, node: Node) {
    if let Some(right) = node.child_by_field_name("right") {
        visit(builder, right);
    }
    if let Some(annotation) = node.child_by_field_name("type") {
        visit(builder, annotation);
    }

    let scope_kind = builder.current_scope_kind();
    if !matches!(scope_kind, ScopeKind::File | ScopeKind::Class) {
        return;
    }
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    let annotation = node
        .child_by_field_name("type")
        .map(|node| builder.text(node));
    let kind = if annotation
        .map(|written| written == "Final" || written.starts_with("Final["))
        .unwrap_or(false)
    {
        DeclarationKind::Constant
    } else if scope_kind == ScopeKind::Class {
        DeclarationKind::Field
    } else {
        DeclarationKind::Variable
    };

    let mut targets: Vec<Node> = Vec::new();
    match left.kind() {
        "identifier" => targets.push(left),
        "pattern_list" => {
            let mut cursor = left.walk();
            for child in left.children(&mut cursor) {
                if child.kind() == "identifier" {
                    targets.push(child);
                }
            }
        }
        _ => return,
    }
    for target in targets {
        builder.push_declaration(DeclarationDraft::new(
            kind,
            builder.text(target).to_string(),
            builder.range(target),
            builder.range(node),
        ));
    }
}

fn emit_call(builder: &mut FactBuilder<'_>, node: Node) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    let form = match function.kind() {
        "identifier" => CallLikeForm::PlainName,
        "attribute" => CallLikeForm::MemberSelector,
        _ => CallLikeForm::Indirect,
    };
    builder.push_call(
        form,
        builder.text(function),
        builder.range(function),
        builder.range(node),
        None,
        false,
        false,
    );
}
