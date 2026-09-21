use serde::{Deserialize, Serialize};

use super::{
    CallLikeOccurrence, Declaration, Diagnostic, DiagnosticKind, DiagnosticSeverity,
    ImportOccurrence, LanguageId, LocalBindingOccurrence, ReferenceOccurrence, Scope, SourceRange,
    TestEvidence,
};

/// Version of the normalized fact schema. Bumped when the shape of the
/// canonical facts changes in a way that is not purely additive.
///
/// * `1` — the original fact set (`scopes`, `declarations`, `imports`,
///   `references`, `calls`, `diagnostics`, `recovery_regions`).
/// * `2` — added `bindings` (`LocalBindingOccurrence`), the Rust local
///   name-binding facts with bounded visibility ranges.
/// * `3` — `BindingKind` gained the Go local-binding variants
///   (`short_variable`, `variable`, `constant`, `local_type`, `function_result`,
///   `method_receiver`, `function_literal_parameter`, `function_literal_result`,
///   `range_variable`, `type_switch_variable`, `select_receive_variable`), which
///   changes the serialized `bindings` schema.
pub const SCHEMA_VERSION: u32 = 3;

/// The analyzed source snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    /// Parsed with no `ERROR` and no `MISSING` nodes.
    Clean,
    /// Parsed, but the tree contains recovery artifacts (`ERROR`/`MISSING`).
    Recovered,
    /// Parsed, but at least one analysis step could not run to completion, so
    /// the facts are known to be partial. Today this means a Tree-sitter query
    /// hit its match limit and captures were abandoned.
    Incomplete,
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
            AnalysisStatus::Incomplete => "incomplete",
            AnalysisStatus::Unsupported => "unsupported",
            AnalysisStatus::Failed => "failed",
        }
    }
}

/// Complete normalized analysis of a single source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Local name bindings ordered by byte range. `binding_id` is the index.
    ///
    /// These are *syntax* facts: a written identifier introduced by a `let`,
    /// parameter, or pattern with the source regions where it may shadow an
    /// outer name. They are not resolved values and not call targets.
    pub bindings: Vec<LocalBindingOccurrence>,
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
            bindings: Vec::new(),
            file_test_evidence: Vec::new(),
            recovery_regions: Vec::new(),
        }
    }

    /// Put every collection into its canonical order and re-index the facts
    /// whose ids are positions.
    ///
    /// This is the single implementation of deterministic normalization. It is
    /// called once, by the builder that produced the analysis, so any
    /// `FileAnalysis` that exists in this crate is already normalized.
    pub fn normalize(&mut self) {
        // References and call-like occurrences are collected by independent
        // mechanisms, so sort them into canonical order and re-index.
        self.references.sort_by(|left, right| {
            (
                left.range.byte_start,
                left.range.byte_end,
                left.kind,
                left.written.as_str(),
            )
                .cmp(&(
                    right.range.byte_start,
                    right.range.byte_end,
                    right.kind,
                    right.written.as_str(),
                ))
        });
        for (index, reference) in self.references.iter_mut().enumerate() {
            reference.reference_id = index as u32;
        }
        self.calls.sort_by(|left, right| {
            (
                left.expression_range.byte_start,
                left.expression_range.byte_end,
                left.form,
                left.callee_written.as_str(),
            )
                .cmp(&(
                    right.expression_range.byte_start,
                    right.expression_range.byte_end,
                    right.form,
                    right.callee_written.as_str(),
                ))
        });
        for (index, call) in self.calls.iter_mut().enumerate() {
            call.call_id = index as u32;
        }
        self.bindings.sort_by(|left, right| {
            (
                left.name_range.byte_start,
                left.name_range.byte_end,
                left.kind,
                left.name.as_str(),
            )
                .cmp(&(
                    right.name_range.byte_start,
                    right.name_range.byte_end,
                    right.kind,
                    right.name.as_str(),
                ))
        });
        for (index, binding) in self.bindings.iter_mut().enumerate() {
            binding.binding_id = index as u32;
        }
        self.diagnostics.sort_by(|left, right| {
            let left_start = left.range.map(|range| range.byte_start).unwrap_or(u32::MAX);
            let right_start = right
                .range
                .map(|range| range.byte_start)
                .unwrap_or(u32::MAX);
            (left_start, left.kind, left.message.as_str()).cmp(&(
                right_start,
                right.kind,
                right.message.as_str(),
            ))
        });
        self.file_test_evidence.sort_by(|left, right| {
            (left.range.byte_start, left.kind, left.detail.as_str()).cmp(&(
                right.range.byte_start,
                right.kind,
                right.detail.as_str(),
            ))
        });
        self.recovery_regions
            .sort_by_key(|range| (range.byte_start, range.byte_end));
        self.recovery_regions.dedup();
    }

    /// A normalized clone.
    pub fn normalized(&self) -> Self {
        let mut clone = self.clone();
        clone.normalize();
        clone
    }

    /// Authoritative fact equality.
    ///
    /// Compares **every** normalized field of the analysis, including the
    /// row/column coordinates, the ranges nested inside import items and
    /// type arguments, and the ranges carried by test evidence. Determinism and
    /// incremental-equivalence claims rest on this, not on the canonical text.
    pub fn same_facts(&self, other: &Self) -> bool {
        self == other
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

    /// All local bindings with the given written name, in canonical order.
    ///
    /// This is a bounded exact lookup for tests and consumers, not a
    /// generalized search and not name resolution.
    pub fn bindings_named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a LocalBindingOccurrence> {
        self.bindings.iter().filter(move |b| b.name == name)
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
    /// analysis.
    ///
    /// This is a *complete* rendering: every field that
    /// [`FileAnalysis::same_facts`] compares appears here, including row/column
    /// coordinates, import item ranges, type-argument ranges and test-evidence
    /// ranges. The digest and incremental-equivalence machinery rely on that,
    /// and `tests/canonical_completeness.rs` enforces it field by field.
    ///
    /// Two analyses with the same canonical text carry the same facts.
    pub fn canonical_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!(
            "file schema={} path={} lang={} bytes={} lines={} final_newline={} status={} snapshot={}",
            self.schema_version,
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
                "scope {} kind={} parent={} name={} range={} header={} lang={}",
                scope.scope_id,
                scope.kind.as_str(),
                scope
                    .parent_scope_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                escape_field(scope.name.as_deref().unwrap_or("-")),
                scope.range.render(),
                scope
                    .header_range
                    .as_ref()
                    .map(SourceRange::render)
                    .unwrap_or_else(|| "-".to_string()),
                scope.language.as_str(),
            ));
        }
        for declaration in &self.declarations {
            lines.push(format!(
                "decl {} kind={} name={} scope={} flags=[{}] range={} name_range={} body={} \
                 lang={} path={} snapshot={} tests=[{}]",
                declaration.declaration_id,
                declaration.kind.as_str(),
                escape_field(&declaration.name),
                declaration.scope_id,
                render_flags(declaration),
                declaration.range.render(),
                declaration.name_range.render(),
                declaration
                    .body_range
                    .as_ref()
                    .map(SourceRange::render)
                    .unwrap_or_else(|| "-".to_string()),
                declaration.language.as_str(),
                escape_field(&declaration.relative_path),
                declaration.snapshot_id,
                render_tests(&declaration.test_evidence),
            ));
        }
        for import in &self.imports {
            let items = import
                .items
                .iter()
                .map(|item| {
                    format!(
                        "{{target={} target_range={} alias={} alias_range={} wildcard={} cat={} range={}}}",
                        escape_list_item(&item.target),
                        item.target_range.render(),
                        escape_list_item(item.alias.as_deref().unwrap_or("-")),
                        item.alias_range
                            .as_ref()
                            .map(SourceRange::render)
                            .unwrap_or_else(|| "-".to_string()),
                        item.wildcard,
                        item.category.as_str(),
                        item.range.render(),
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            lines.push(format!(
                "import {} form={} module={} module_range={} relative={} scope={} range={} \
                 lang={} path={} snapshot={} text={} items=[{}]",
                import.import_id,
                import.form.as_str(),
                escape_list_item(import.module.as_deref().unwrap_or("-")),
                import
                    .module_range
                    .as_ref()
                    .map(SourceRange::render)
                    .unwrap_or_else(|| "-".to_string()),
                import
                    .relative_levels
                    .map(|levels| levels.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                import.scope_id,
                import.statement_range.render(),
                import.language.as_str(),
                escape_field(&import.relative_path),
                import.snapshot_id,
                escape_field(&import.statement_text),
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
                reference.range.render(),
            ));
        }
        for call in &self.calls {
            lines.push(format!(
                "call {} form={} callee={} scope={} expr={} callee_range={} type_args={} \
                 type_args_range={} dyn={} nullsafe={}",
                call.call_id,
                call.form.as_str(),
                escape_field(&call.callee_written),
                call.scope_id,
                call.expression_range.render(),
                call.callee_range.render(),
                escape_field(call.type_arguments.as_deref().unwrap_or("-")),
                call.type_arguments_range
                    .as_ref()
                    .map(SourceRange::render)
                    .unwrap_or_else(|| "-".to_string()),
                call.dynamic_callee,
                call.nullsafe,
            ));
        }
        for binding in &self.bindings {
            let visibility = binding
                .visibility_ranges
                .iter()
                .map(SourceRange::render)
                .collect::<Vec<_>>()
                .join(",");
            lines.push(format!(
                "binding {} kind={} name={} scope={} name_range={} site={} vis=[{}] ambiguous={} \
                 lang={} path={} snapshot={}",
                binding.binding_id,
                binding.kind.as_str(),
                escape_field(&binding.name),
                binding.scope_id,
                binding.name_range.render(),
                binding.binding_site_range.render(),
                visibility,
                binding.ambiguous,
                binding.language.as_str(),
                escape_field(&binding.relative_path),
                binding.snapshot_id,
            ));
        }
        for evidence in &self.file_test_evidence {
            lines.push(format!(
                "file_test kind={} detail={} range={}",
                evidence.kind.as_str(),
                escape_field(&evidence.detail),
                evidence.range.render()
            ));
        }
        for region in &self.recovery_regions {
            lines.push(format!("recovery range={}", region.render()));
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
        .map(|item| {
            format!(
                "{}:{}:{}",
                item.kind.as_str(),
                escape_list_item(&item.detail),
                item.range.render()
            )
        })
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
