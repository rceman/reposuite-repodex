use tree_sitter::Node;

use crate::model::{
    AnalysisStatus, BindingKind, CallLikeForm, CallLikeOccurrence, Declaration, DeclarationFlag,
    DeclarationKind, Diagnostic, FileAnalysis, ImportCategory, ImportForm, ImportItem,
    ImportOccurrence, LanguageId, LineIndex, LocalBindingOccurrence, ReferenceKind,
    ReferenceOccurrence, Scope, ScopeKind, SourceFile, SourceRange, TestEvidence, SCHEMA_VERSION,
};

/// A declaration under construction.
#[derive(Debug, Clone)]
pub struct DeclarationDraft {
    pub kind: DeclarationKind,
    pub name: String,
    pub name_range: SourceRange,
    pub range: SourceRange,
    pub body_range: Option<SourceRange>,
    pub flags: Vec<DeclarationFlag>,
}

impl DeclarationDraft {
    pub fn new(
        kind: DeclarationKind,
        name: impl Into<String>,
        name_range: SourceRange,
        range: SourceRange,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            name_range,
            range,
            body_range: None,
            flags: Vec::new(),
        }
    }

    pub fn with_body(mut self, body_range: Option<SourceRange>) -> Self {
        self.body_range = body_range;
        self
    }

    pub fn with_flag(mut self, flag: DeclarationFlag) -> Self {
        if !self.flags.contains(&flag) {
            self.flags.push(flag);
        }
        self
    }
}

/// Accumulates normalized facts for one file.
///
/// Adapters push facts in pre-order traversal order. Scope ids, declaration ids,
/// import ids, reference ids and call ids are indices into the corresponding
/// vectors, which makes them deterministic and free of randomness.
pub struct FactBuilder<'a> {
    source: &'a [u8],
    /// Line-start table built once per file. Derived ranges use it so that
    /// extraction stays linear in the file size.
    lines: LineIndex,
    file: SourceFile,
    status: AnalysisStatus,
    scopes: Vec<Scope>,
    declarations: Vec<Declaration>,
    imports: Vec<ImportOccurrence>,
    references: Vec<ReferenceOccurrence>,
    calls: Vec<CallLikeOccurrence>,
    bindings: Vec<LocalBindingOccurrence>,
    diagnostics: Vec<Diagnostic>,
    recovery_regions: Vec<SourceRange>,
    file_test_evidence: Vec<TestEvidence>,
    scope_stack: Vec<u32>,
}

impl<'a> FactBuilder<'a> {
    pub fn new(relative_path: impl Into<String>, language: LanguageId, source: &'a [u8]) -> Self {
        let file = SourceFile::new(relative_path, language, source);
        let lines = LineIndex::new(source);
        Self {
            source,
            lines,
            file,
            status: AnalysisStatus::Clean,
            scopes: Vec::new(),
            declarations: Vec::new(),
            imports: Vec::new(),
            references: Vec::new(),
            calls: Vec::new(),
            bindings: Vec::new(),
            diagnostics: Vec::new(),
            recovery_regions: Vec::new(),
            file_test_evidence: Vec::new(),
            scope_stack: Vec::new(),
        }
    }

    pub fn source(&self) -> &'a [u8] {
        self.source
    }

    pub fn file(&self) -> &SourceFile {
        &self.file
    }

    pub fn language(&self) -> LanguageId {
        self.file.language
    }

    /// Source text of a node. The returned slice borrows from the source
    /// buffer, not from the builder, so adapters can keep it across mutations.
    pub fn text(&self, node: Node) -> &'a str {
        self.text_range(self.range(node))
    }

    pub fn text_range(&self, range: SourceRange) -> &'a str {
        let start = (range.byte_start as usize).min(self.source.len());
        let end = (range.byte_end as usize).min(self.source.len());
        std::str::from_utf8(&self.source[start..end]).unwrap_or("")
    }

    /// Range of a derived byte span, using the precomputed line index.
    pub fn range_from_offsets(&self, byte_start: u32, byte_end: u32) -> SourceRange {
        self.lines.range(byte_start, byte_end)
    }

    pub fn range(&self, node: Node) -> SourceRange {
        let start = node.start_position();
        let end = node.end_position();
        SourceRange::new(
            node.start_byte() as u32,
            node.end_byte() as u32,
            start.row as u32,
            start.column as u32,
            end.row as u32,
            end.column as u32,
        )
    }

    pub fn scope_id(&self) -> u32 {
        *self
            .scope_stack
            .last()
            .expect("adapter must push a file scope before emitting facts")
    }

    /// Kind of the scope currently being filled.
    pub fn current_scope_kind(&self) -> ScopeKind {
        self.scopes[self.scope_id() as usize].kind
    }

    /// Push a scope and make it current.
    pub fn push_scope(
        &mut self,
        kind: ScopeKind,
        name: Option<impl Into<String>>,
        range: SourceRange,
        header_range: Option<SourceRange>,
    ) -> u32 {
        let scope_id = self.scopes.len() as u32;
        let parent_scope_id = self.scope_stack.last().copied();
        self.scopes.push(Scope {
            scope_id,
            parent_scope_id,
            kind,
            name: name.map(Into::into),
            range,
            header_range,
            language: self.file.language,
        });
        self.scope_stack.push(scope_id);
        scope_id
    }

    /// Pop the current scope. The file scope must not be popped while facts are
    /// still being emitted.
    pub fn pop_scope(&mut self) {
        self.scope_stack.pop();
    }

    pub fn push_declaration(&mut self, draft: DeclarationDraft) -> u32 {
        let declaration_id = self.declarations.len() as u32;
        self.declarations.push(Declaration {
            declaration_id,
            snapshot_id: self.file.snapshot_id.clone(),
            language: self.file.language,
            relative_path: self.file.relative_path.clone(),
            scope_id: self.scope_id(),
            kind: draft.kind,
            name: draft.name,
            name_range: draft.name_range,
            range: draft.range,
            body_range: draft.body_range,
            flags: draft.flags,
            test_evidence: Vec::new(),
        });
        declaration_id
    }

    pub fn declaration_mut(&mut self, declaration_id: u32) -> &mut Declaration {
        &mut self.declarations[declaration_id as usize]
    }

    pub fn add_flag(&mut self, declaration_id: u32, flag: DeclarationFlag) {
        let declaration = &mut self.declarations[declaration_id as usize];
        if !declaration.flags.contains(&flag) {
            declaration.flags.push(flag);
        }
    }

    pub fn add_test_evidence(&mut self, declaration_id: u32, evidence: TestEvidence) {
        let declaration = &mut self.declarations[declaration_id as usize];
        if !declaration.test_evidence.contains(&evidence) {
            declaration.test_evidence.push(evidence);
        }
    }

    pub fn add_file_test_evidence(&mut self, evidence: TestEvidence) {
        if !self.file_test_evidence.contains(&evidence) {
            self.file_test_evidence.push(evidence);
        }
    }

    pub fn push_import(
        &mut self,
        form: ImportForm,
        module: Option<String>,
        module_range: Option<SourceRange>,
        relative_levels: Option<u32>,
        items: Vec<ImportItem>,
        statement_range: SourceRange,
    ) -> u32 {
        let import_id = self.imports.len() as u32;
        self.imports.push(ImportOccurrence {
            import_id,
            snapshot_id: self.file.snapshot_id.clone(),
            language: self.file.language,
            relative_path: self.file.relative_path.clone(),
            scope_id: self.scope_id(),
            form,
            module,
            module_range,
            relative_levels,
            items,
            statement_range,
            statement_text: self.text_range(statement_range).to_string(),
        });
        import_id
    }

    pub fn push_reference(
        &mut self,
        kind: ReferenceKind,
        written: impl Into<String>,
        range: SourceRange,
        declaration_id: Option<u32>,
    ) -> u32 {
        let reference_id = self.references.len() as u32;
        self.references.push(ReferenceOccurrence {
            reference_id,
            kind,
            written: written.into(),
            range,
            scope_id: self.scope_id(),
            declaration_id,
        });
        reference_id
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_call(
        &mut self,
        form: CallLikeForm,
        callee_written: impl Into<String>,
        callee_range: SourceRange,
        expression_range: SourceRange,
        type_arguments: Option<(String, SourceRange)>,
        dynamic_callee: bool,
        nullsafe: bool,
    ) -> u32 {
        let call_id = self.calls.len() as u32;
        let (type_arguments, type_arguments_range) = match type_arguments {
            Some((text, range)) => (Some(text), Some(range)),
            None => (None, None),
        };
        self.calls.push(CallLikeOccurrence {
            call_id,
            scope_id: self.scope_id(),
            form,
            expression_range,
            callee_range,
            callee_written: callee_written.into(),
            type_arguments,
            type_arguments_range,
            dynamic_callee,
            nullsafe,
        });
        call_id
    }

    /// Push a local name binding. `binding_id` is the position in the bindings
    /// vector. The adapter supplies the visibility ranges; `scope_id` is the
    /// lexically enclosing scope at emission time.
    pub fn push_binding(
        &mut self,
        kind: BindingKind,
        name: impl Into<String>,
        name_range: SourceRange,
        binding_site_range: SourceRange,
        visibility_ranges: Vec<SourceRange>,
        ambiguous: bool,
    ) -> u32 {
        let binding_id = self.bindings.len() as u32;
        self.bindings.push(LocalBindingOccurrence {
            binding_id,
            snapshot_id: self.file.snapshot_id.clone(),
            language: self.file.language,
            relative_path: self.file.relative_path.clone(),
            scope_id: self.scope_id(),
            kind,
            name: name.into(),
            name_range,
            binding_site_range,
            visibility_ranges,
            ambiguous,
        });
        binding_id
    }

    pub fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn mark_recovered(&mut self) {
        if self.status == AnalysisStatus::Clean {
            self.status = AnalysisStatus::Recovered;
        }
    }

    /// Record that an analysis step could not run to completion.
    ///
    /// This outranks [`FactBuilder::mark_recovered`]: an analysis whose facts
    /// are known to be partial must never be reported as merely `Recovered`,
    /// and it must never be reported as `Clean`.
    pub fn mark_incomplete(&mut self) {
        self.status = AnalysisStatus::Incomplete;
    }

    pub fn push_recovery_region(&mut self, range: SourceRange) {
        self.recovery_regions.push(range);
    }

    pub fn recovery_regions(&self) -> &[SourceRange] {
        &self.recovery_regions
    }

    /// Consume the builder and produce the canonical file analysis.
    ///
    /// Normalization itself lives on [`FileAnalysis::normalize`], so there is
    /// exactly one implementation of deterministic ordering in the crate.
    pub fn finish(self) -> FileAnalysis {
        let mut analysis = FileAnalysis {
            schema_version: SCHEMA_VERSION,
            file: self.file,
            status: self.status,
            diagnostics: self.diagnostics,
            scopes: self.scopes,
            declarations: self.declarations,
            imports: self.imports,
            references: self.references,
            calls: self.calls,
            bindings: self.bindings,
            file_test_evidence: self.file_test_evidence,
            recovery_regions: self.recovery_regions,
        };
        analysis.normalize();
        analysis
    }
}

/// Build an import item from raw parts.
pub fn import_item(
    target: impl Into<String>,
    target_range: SourceRange,
    alias: Option<(String, SourceRange)>,
    wildcard: bool,
    category: ImportCategory,
    range: SourceRange,
) -> ImportItem {
    let (alias, alias_range) = match alias {
        Some((alias, alias_range)) => (Some(alias), Some(alias_range)),
        None => (None, None),
    };
    ImportItem {
        target: target.into(),
        target_range,
        alias,
        alias_range,
        wildcard,
        category,
        range,
    }
}
