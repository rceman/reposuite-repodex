use serde::Serialize;

use super::{
    CallLikeOccurrence, Declaration, Diagnostic, DiagnosticKind, DiagnosticSeverity,
    ImportOccurrence, LanguageId, ReferenceOccurrence, Scope, SourceRange, TestEvidence,
};

/// Version of the normalized fact schema. Bumped when the shape of the
/// canonical facts changes in a way that is not purely additive.
pub const SCHEMA_VERSION: u32 = 1;

/// The analyzed source snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceFile {
    /// Local snapshot identifier. Derived from the file bytes only, so it is
    /// stable across checkouts, machines and discovery order.
    pub snapshot_id: String,
    /// Path relative to the analysis root, always `/`-separated. Absolute
    /// checkout paths never appear in normalized facts.
    pub relative_path: String,
    pub language: LanguageId,
    pub byte_len: u32,
    /// Number of lines. A trailing newline does not create an extra empty line.
    pub line_count: u32,
    /// Whether the source ends with a newline byte.
    pub has_final_newline: bool,
}

impl SourceFile {
    pub fn new(relative_path: impl Into<String>, language: LanguageId, source: &[u8]) -> Self {
        Self {
            snapshot_id: snapshot_id(source),
            relative_path: relative_path.into(),
            language,
            byte_len: source.len() as u32,
            line_count: line_count(source),
            has_final_newline: source.last() == Some(&b'\n'),
        }
    }
}

/// Outcome of analyzing one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    /// Parsed with no `ERROR` and no `MISSING` nodes.
    Clean,
    /// Parsed, but the tree contains recovery artifacts (`ERROR`/`MISSING`).
    Recovered,
    /// Not analyzed because the input is unsupported (encoding, size, language).
    Unsupported,
    /// Not analyzed because the input could not be read.
    Failed,
}

impl AnalysisStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AnalysisStatus::Clean => "clean",
            AnalysisStatus::Recovered => "recovered",
            AnalysisStatus::Unsupported => "unsupported",
            AnalysisStatus::Failed => "failed",
        }
    }
}

/// Complete normalized analysis of a single source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileAnalysis {
    pub schema_version: u32,
    pub file: SourceFile,
    pub status: AnalysisStatus,
    pub diagnostics: Vec<Diagnostic>,
    /// Scopes in pre-order. `Scope::scope_id` is the index into this vector.
    pub scopes: Vec<Scope>,
    /// Declarations in pre-order. `Declaration::declaration_id` is the index.
    pub declarations: Vec<Declaration>,
    /// Imports in pre-order. `ImportOccurrence::import_id` is the index.
    pub imports: Vec<ImportOccurrence>,
    /// References ordered by byte range. `reference_id` is the index.
    pub references: Vec<ReferenceOccurrence>,
    /// Call-like occurrences ordered by byte range. `call_id` is the index.
    pub calls: Vec<CallLikeOccurrence>,
    /// File-level test evidence, e.g. a `_test.go` file name convention.
    pub file_test_evidence: Vec<TestEvidence>,
    /// Byte ranges where Tree-sitter had to recover.
    pub recovery_regions: Vec<SourceRange>,
}

impl FileAnalysis {
    /// An analysis that never parsed anything.
    pub fn unsupported(
        relative_path: impl Into<String>,
        language: LanguageId,
        source: &[u8],
        status: AnalysisStatus,
        diagnostic: Diagnostic,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            file: SourceFile::new(relative_path, language, source),
            status,
            diagnostics: vec![diagnostic],
            scopes: Vec::new(),
            declarations: Vec::new(),
            imports: Vec::new(),
            references: Vec::new(),
            calls: Vec::new(),
            file_test_evidence: Vec::new(),
            recovery_regions: Vec::new(),
        }
    }

    pub fn declaration_count(&self) -> usize {
        self.declarations.len()
    }

    /// Declarations carrying at least one piece of test evidence.
    pub fn test_candidate_count(&self) -> usize {
        self.declarations
            .iter()
            .filter(|d| !d.test_evidence.is_empty())
            .count()
    }

    /// True when the analysis contains a diagnostic of the given kind.
    pub fn has_diagnostic(&self, kind: DiagnosticKind) -> bool {
        self.diagnostics.iter().any(|d| d.kind == kind)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count()
    }

    /// Look up a declaration by written name, returning the first match in
    /// canonical order.
    pub fn declaration_named(&self, name: &str) -> Option<&Declaration> {
        self.declarations.iter().find(|d| d.name == name)
    }

    /// All declarations with the given written name, in canonical order.
    pub fn declarations_named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a Declaration> {
        self.declarations.iter().filter(move |d| d.name == name)
    }

    pub fn scope(&self, scope_id: u32) -> &Scope {
        &self.scopes[scope_id as usize]
    }

    /// Chain of scope ids from the file scope down to `scope_id`, inclusive.
    pub fn scope_chain(&self, scope_id: u32) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut current = Some(scope_id);
        while let Some(id) = current {
            chain.push(id);
            current = self.scopes[id as usize].parent_scope_id;
        }
        chain.reverse();
        chain
    }

    /// Human readable scope path such as `file::mod::impl::method`.
    ///
    /// Anonymous scopes render as `(closure)`/`(lambda)`/`(arrow)` placeholders
    /// so that the *display* is readable while the model still stores no
    /// invented name.
    pub fn scope_path(&self, scope_id: u32) -> String {
        self.scope_chain(scope_id)
            .into_iter()
            .map(|id| {
                let scope = &self.scopes[id as usize];
                match (&scope.name, scope.kind) {
                    (Some(name), _) => name.clone(),
                    (None, kind) => {
                        // `impl` scopes carry no name by design; the display
                        // borrows the written target from the impl references
                        // so that two impl blocks stay distinguishable.
                        if kind == super::ScopeKind::Impl {
                            let target = self
                                .references
                                .iter()
                                .find(|reference| {
                                    reference.scope_id == id
                                        && reference.kind == super::ReferenceKind::ImplTarget
                                })
                                .map(|reference| reference.written.clone());
                            let trait_name = self
                                .references
                                .iter()
                                .find(|reference| {
                                    reference.scope_id == id
                                        && reference.kind == super::ReferenceKind::ImplTrait
                                })
                                .map(|reference| reference.written.clone());
                            match (trait_name, target) {
                                (Some(trait_name), Some(target)) => {
                                    format!("(impl {trait_name} for {target})")
                                }
                                (None, Some(target)) => format!("(impl {target})"),
                                _ => "(impl)".to_string(),
                            }
                        } else {
                            format!("({})", kind.as_str())
                        }
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("::")
    }

    /// Canonical, deterministic rendering of every normalized fact in this
    /// analysis. Two analyses with the same canonical text carry the same facts.
    ///
    /// The rendering intentionally uses byte offsets only; row/column
    /// correctness is asserted separately by range tests.
    pub fn canonical_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!(
            "file path={} lang={} bytes={} lines={} final_newline={} status={} snapshot={}",
            escape_field(&self.file.relative_path),
            self.file.language.as_str(),
            self.file.byte_len,
            self.file.line_count,
            self.file.has_final_newline,
            self.status.as_str(),
            self.file.snapshot_id
        ));
        for scope in &self.scopes {
            lines.push(format!(
                "scope {} kind={} parent={} name={} range={} header={}",
                scope.scope_id,
                scope.kind.as_str(),
                scope
                    .parent_scope_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                escape_field(scope.name.as_deref().unwrap_or("-")),
                render_range(&scope.range),
                scope
                    .header_range
                    .as_ref()
                    .map(render_range)
                    .unwrap_or_else(|| "-".to_string()),
            ));
        }
        for declaration in &self.declarations {
            lines.push(format!(
                "decl {} kind={} name={} scope={} flags=[{}] range={} name_range={} body={} tests=[{}]",
                declaration.declaration_id,
                declaration.kind.as_str(),
                escape_field(&declaration.name),
                declaration.scope_id,
                render_flags(declaration),
                render_range(&declaration.range),
                render_range(&declaration.name_range),
                declaration
                    .body_range
                    .as_ref()
                    .map(render_range)
                    .unwrap_or_else(|| "-".to_string()),
                render_tests(&declaration.test_evidence),
            ));
        }
        for import in &self.imports {
            let items = import
                .items
                .iter()
                .map(|item| {
                    format!(
                        "{{target={} alias={} wildcard={} cat={}}}",
                        escape_list_item(&item.target),
                        escape_list_item(item.alias.as_deref().unwrap_or("-")),
                        item.wildcard,
                        item.category.as_str(),
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            lines.push(format!(
                "import {} form={} module={} module_range={} relative={} scope={} range={} items=[{}]",
                import.import_id,
                import.form.as_str(),
                escape_list_item(import.module.as_deref().unwrap_or("-")),
                import
                    .module_range
                    .as_ref()
                    .map(render_range)
                    .unwrap_or_else(|| "-".to_string()),
                import
                    .relative_levels
                    .map(|levels| levels.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                import.scope_id,
                render_range(&import.statement_range),
                items,
            ));
        }
        for reference in &self.references {
            lines.push(format!(
                "ref {} kind={} written={} scope={} decl={} range={}",
                reference.reference_id,
                reference.kind.as_str(),
                escape_field(&reference.written),
                reference.scope_id,
                reference
                    .declaration_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                render_range(&reference.range),
            ));
        }
        for call in &self.calls {
            lines.push(format!(
                "call {} form={} callee={} scope={} expr={} callee_range={} type_args={} dyn={} nullsafe={}",
                call.call_id,
                call.form.as_str(),
                escape_field(&call.callee_written),
                call.scope_id,
                render_range(&call.expression_range),
                render_range(&call.callee_range),
                escape_field(call.type_arguments.as_deref().unwrap_or("-")),
                call.dynamic_callee,
                call.nullsafe,
            ));
        }
        for evidence in &self.file_test_evidence {
            lines.push(format!(
                "file_test kind={} detail={} range={}",
                evidence.kind.as_str(),
                escape_field(&evidence.detail),
                render_range(&evidence.range)
            ));
        }
        for region in &self.recovery_regions {
            lines.push(format!("recovery range={}", render_range(region)));
        }
        for diagnostic in &self.diagnostics {
            lines.push(format!("diag {}", escape_field(&diagnostic.render())));
        }
        lines
    }

    /// Canonical text: every normalized fact, one per line.
    pub fn canonical_text(&self) -> String {
        let mut text = self.canonical_lines().join("\n");
        text.push('\n');
        text
    }
}

/// Canonical facts are one fact per line, so free text must never contain a
/// raw newline. Backslashes are escaped first so the mapping stays injective.
fn escape_field(value: &str) -> String {
    if !value.contains(['\\', '\n', '\r', '\t']) {
        return value.to_string();
    }
    let mut escaped = String::with_capacity(value.len() + 8);
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Same as [`escape_field`], plus `,`, so that a value can be embedded in a
/// comma-separated list without ambiguity.
fn escape_list_item(value: &str) -> String {
    escape_field(value).replace(',', "\\,")
}

fn render_range(range: &SourceRange) -> String {
    format!("{}..{}", range.byte_start, range.byte_end)
}

fn render_flags(declaration: &Declaration) -> String {
    declaration
        .flags
        .iter()
        .map(|flag| flag.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn render_tests(evidence: &[TestEvidence]) -> String {
    evidence
        .iter()
        .map(|item| format!("{}:{}", item.kind.as_str(), escape_list_item(&item.detail)))
        .collect::<Vec<_>>()
        .join(",")
}

/// FNV-1a 64-bit hash. Small, deterministic and dependency-free.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Content-derived snapshot identifier for a source buffer.
pub fn snapshot_id(source: &[u8]) -> String {
    format!("snap-{:016x}", fnv1a64(source))
}

/// Number of lines, where a trailing newline does not create an extra line.
pub fn line_count(source: &[u8]) -> u32 {
    if source.is_empty() {
        return 0;
    }
    let newlines = source.iter().filter(|byte| **byte == b'\n').count() as u32;
    if source.last() == Some(&b'\n') {
        newlines
    } else {
        newlines + 1
    }
}
