//! Tree-sitter incremental parsing correctness harness.
//!
//! This is *incremental syntax parsing*, not incremental repository indexing.
//!
//! ```text
//! parse original source
//!     -> construct edited source
//!     -> compute InputEdit
//!     -> edit the previous tree
//!     -> incrementally parse the new bytes
//!     -> independently full-parse the same bytes
//!     -> extract complete normalized facts from both trees
//!     -> compare
//! ```
//!
//! After every edit the harness re-extracts the *complete* edited file. There is
//! no fine-grained fact patching, and `changed_ranges` is recorded as
//! observation only -- it is never used as the fact invalidation mechanism.

use tree_sitter::{InputEdit, Parser, Point, Tree};

use crate::model::{FileAnalysis, SourceRange};
use crate::parser::{extract_from_tree, LanguageAdapter};

/// A single contiguous text replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_text: String,
}

impl TextEdit {
    pub fn replace(start_byte: usize, old_end_byte: usize, new_text: impl Into<String>) -> Self {
        Self {
            start_byte,
            old_end_byte,
            new_text: new_text.into(),
        }
    }

    /// Insert `text` before `byte_offset`.
    pub fn insert(byte_offset: usize, text: impl Into<String>) -> Self {
        Self::replace(byte_offset, byte_offset, text)
    }

    /// Delete `[start_byte, old_end_byte)`.
    pub fn delete(start_byte: usize, old_end_byte: usize) -> Self {
        Self::replace(start_byte, old_end_byte, "")
    }

    pub fn apply(&self, source: &str) -> String {
        let mut result = String::with_capacity(source.len() + self.new_text.len());
        result.push_str(&source[..self.start_byte]);
        result.push_str(&self.new_text);
        result.push_str(&source[self.old_end_byte..]);
        result
    }

    /// The Tree-sitter [`InputEdit`] describing this replacement.
    pub fn to_input_edit(&self, old_source: &str, new_source: &str) -> InputEdit {
        let new_end_byte = self.start_byte + self.new_text.len();
        InputEdit {
            start_byte: self.start_byte,
            old_end_byte: self.old_end_byte,
            new_end_byte,
            start_position: point_of(old_source, self.start_byte),
            old_end_position: point_of(old_source, self.old_end_byte),
            new_end_position: point_of(new_source, new_end_byte),
        }
    }
}

/// Zero-based `(row, column)` with UTF-8 byte columns.
fn point_of(source: &str, byte_offset: usize) -> Point {
    let bytes = source.as_bytes();
    let offset = byte_offset.min(bytes.len());
    let mut row = 0usize;
    let mut line_start = 0usize;
    for (index, byte) in bytes.iter().enumerate().take(offset) {
        if *byte == b'\n' {
            row += 1;
            line_start = index + 1;
        }
    }
    Point {
        row,
        column: offset - line_start,
    }
}

/// Result of one incremental-equivalence case.
#[derive(Debug, Clone)]
pub struct IncrementalOutcome {
    pub tree_structure_equal: bool,
    pub facts_equal: bool,
    pub diagnostics_equal: bool,
    pub recovery_equal: bool,
    /// Byte ranges Tree-sitter reported as changed. Observation only.
    pub changed_ranges: Vec<SourceRange>,
    pub details: Vec<String>,
    pub incremental: FileAnalysis,
    pub full: FileAnalysis,
}

impl IncrementalOutcome {
    pub fn equivalent(&self) -> bool {
        self.tree_structure_equal
            && self.facts_equal
            && self.diagnostics_equal
            && self.recovery_equal
    }
}

/// Run one incremental-equivalence case.
pub fn run_case(
    adapter: &dyn LanguageAdapter,
    parser: &mut Parser,
    relative_path: &str,
    original: &str,
    edit: &TextEdit,
) -> Result<IncrementalOutcome, String> {
    let mut outcomes = run_sequence(
        adapter,
        parser,
        relative_path,
        original,
        std::slice::from_ref(edit),
    )?;
    Ok(outcomes.remove(0))
}

/// Run a chain of edits, each measured against the tree produced by the
/// previous step.
///
/// Step *n* edits the incrementally parsed tree from step *n-1*, incrementally
/// parses the new bytes and independently full-parses the same bytes. Both trees
/// are then re-extracted in full.
pub fn run_sequence(
    adapter: &dyn LanguageAdapter,
    parser: &mut Parser,
    relative_path: &str,
    original: &str,
    edits: &[TextEdit],
) -> Result<Vec<IncrementalOutcome>, String> {
    let language = adapter.ts_language();
    parser
        .set_language(&language)
        .map_err(|error| format!("set_language failed: {error}"))?;

    let mut current_source = original.to_string();
    let mut current_tree = parser
        .parse(&current_source, None)
        .ok_or_else(|| "tree-sitter returned no tree for the original source".to_string())?;

    let mut outcomes = Vec::with_capacity(edits.len());
    for edit in edits {
        let edited_source = edit.apply(&current_source);
        let input_edit = edit.to_input_edit(&current_source, &edited_source);

        let mut edited_tree = current_tree.clone();
        edited_tree.edit(&input_edit);

        let incremental_tree = parser
            .parse(edited_source.as_bytes(), Some(&edited_tree))
            .ok_or_else(|| "tree-sitter returned no incremental tree".to_string())?;
        let full_tree = parser
            .parse(edited_source.as_bytes(), None)
            .ok_or_else(|| "tree-sitter returned no full tree".to_string())?;

        let changed_ranges = edited_tree
            .changed_ranges(&incremental_tree)
            .map(|range| SourceRange {
                byte_start: range.start_byte as u32,
                byte_end: range.end_byte as u32,
                row_start: range.start_point.row as u32,
                column_start: range.start_point.column as u32,
                row_end: range.end_point.row as u32,
                column_end: range.end_point.column as u32,
            })
            .collect::<Vec<_>>();

        let structure_incremental = tree_structure_digest(incremental_tree.root_node());
        let structure_full = tree_structure_digest(full_tree.root_node());
        let tree_structure_equal = structure_incremental == structure_full;

        let incremental = extract_from_tree(
            adapter,
            relative_path,
            edited_source.as_bytes(),
            &incremental_tree,
        );
        let full = extract_from_tree(adapter, relative_path, edited_source.as_bytes(), &full_tree);

        let facts_equal = incremental.canonical_text() == full.canonical_text();
        let diagnostics_equal = render_diagnostics(&incremental) == render_diagnostics(&full);
        let recovery_equal = incremental.recovery_regions == full.recovery_regions
            && incremental.status == full.status;

        let mut details = Vec::new();
        if !tree_structure_equal {
            details.push("tree structure diverged between incremental and full parse".to_string());
        }
        if !facts_equal {
            details.push(first_diff(
                &incremental.canonical_lines(),
                &full.canonical_lines(),
            ));
        }
        if !diagnostics_equal {
            details.push("diagnostics diverged".to_string());
        }
        if !recovery_equal {
            details.push("recovery state diverged".to_string());
        }

        outcomes.push(IncrementalOutcome {
            tree_structure_equal,
            facts_equal,
            diagnostics_equal,
            recovery_equal,
            changed_ranges,
            details,
            incremental,
            full,
        });

        current_source = edited_source;
        current_tree = incremental_tree;
    }
    Ok(outcomes)
}

/// Structural digest of a tree: node kind, range, namedness and recovery flags
/// for every node, in traversal order. Stronger than S-expression equality
/// because it also compares ranges and ERROR/MISSING state.
pub fn tree_structure_digest(root: tree_sitter::Node<'_>) -> String {
    let mut out = String::new();
    append_structure(root, 0, &mut out);
    out
}

fn append_structure(node: tree_sitter::Node<'_>, depth: usize, out: &mut String) {
    use std::fmt::Write;
    let flags = match (node.is_error(), node.is_missing(), node.has_error()) {
        (true, _, _) => "E",
        (_, true, _) => "M",
        (_, _, true) => "e",
        _ => "-",
    };
    let _ = writeln!(
        out,
        "{depth}:{}:{}..{}:{}:{}",
        node.kind(),
        node.start_byte(),
        node.end_byte(),
        flags,
        node.child_count()
    );
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        append_structure(child, depth + 1, out);
    }
}

fn render_diagnostics(analysis: &FileAnalysis) -> String {
    analysis
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.render())
        .collect::<Vec<_>>()
        .join("\n")
}

fn first_diff(expected: &[String], actual: &[String]) -> String {
    for (index, line) in expected.iter().enumerate() {
        match actual.get(index) {
            Some(other) if other == line => {}
            Some(other) => {
                return format!("fact line {index} differs: incremental `{line}` vs full `{other}`")
            }
            None => return format!("full parse is missing fact line {index}: `{line}`"),
        }
    }
    if actual.len() > expected.len() {
        return format!(
            "full parse has extra fact line: `{}`",
            actual[expected.len()]
        );
    }
    "canonical fact sets differ".to_string()
}

/// Clone a tree so a case can start from the same original state every time.
///
/// Benchmark samples must reset to the same original source/tree state before
/// every measurement; measuring a chain of progressively different documents is
/// not a repeated measurement of one workload.
pub fn reset_tree(tree: &Tree) -> Tree {
    tree.clone()
}
