//! Grammar-level node enumerator for the TASK 2 occurrence audit.
//!
//! Parses one file with the pinned Tree-sitter grammar and prints every *named*
//! node whose kind matches an optional substring filter, with its exact byte
//! span. It deliberately does **not** call the language adapters, so its output
//! is independent of RepoDex's extraction decisions: it shows what the grammar
//! actually contains, which is the ground truth for "syntactic occurrence".
//!
//! ```bash
//! cargo run --release --example grammar_enum -- <file> [kind-substring ...]
//! ```
//!
//! Output is tab-separated:
//!
//! ```text
//! kind  byte_start  byte_end  row_start  col_start  row_end  col_end  parent_kind  field  anchor_start  anchor_end  slice
//! ```
//!
//! `anchor_*` is the node's `name` field child range when the node has one
//! (declaration name anchor), otherwise the node's own range (call whole
//! expression / import statement anchor).

use std::path::PathBuf;

use repodex::model::LanguageId;
use repodex::parser::ParserRegistry;
use tree_sitter::{Node, Parser};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("usage: grammar_enum <file> [kind-substring ...]");
        std::process::exit(2);
    };
    let filters: Vec<String> = args.collect();

    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let Some(language) = LanguageId::from_extension(extension) else {
        eprintln!("unsupported extension: {extension:?}");
        std::process::exit(2);
    };
    let source = std::fs::read(&path).expect("read source");
    let registry = ParserRegistry::new().expect("registry");
    let adapter = registry.adapter(language);

    let mut parser = Parser::new();
    parser
        .set_language(&adapter.ts_language())
        .expect("grammar must load");
    let tree = parser.parse(&source, None).expect("parse");

    let mut cursor = tree.walk();
    walk(&mut cursor, &source, &filters, None, None);
}

fn walk(
    cursor: &mut tree_sitter::TreeCursor<'_>,
    source: &[u8],
    filters: &[String],
    parent: Option<&str>,
    field: Option<&str>,
) {
    let node = cursor.node();
    let kind = node.kind();
    if node.is_named() && (filters.is_empty() || filters.iter().any(|f| kind.contains(f.as_str())))
    {
        let start = node.start_byte();
        let end = node.end_byte();
        let slice = String::from_utf8_lossy(&source[start..end.min(source.len())]);
        let slice = slice.replace(['\n', '\t'], " ");
        let slice: String = slice.chars().take(70).collect();
        let start_pos = node.start_position();
        let end_pos = node.end_position();
        let (anchor_start, anchor_end) = match node.child_by_field_name("name") {
            Some(name) => (name.start_byte(), name.end_byte()),
            None => (start, end),
        };
        println!(
            "{kind}\t{start}\t{end}\t{}\t{}\t{}\t{}\t{}\t{}\t{anchor_start}\t{anchor_end}\t{}",
            start_pos.row,
            start_pos.column,
            end_pos.row,
            end_pos.column,
            parent.unwrap_or("-"),
            field.unwrap_or("-"),
            slice
        );
    }
    if cursor.goto_first_child() {
        loop {
            let child_field = cursor.field_name().map(|name| name.to_string());
            walk(cursor, source, filters, Some(kind), child_field.as_deref());
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

#[allow(dead_code)]
fn node_depth(node: Node<'_>) -> usize {
    let mut depth = 0;
    let mut current = Some(node);
    while let Some(n) = current {
        depth += 1;
        current = n.parent();
    }
    depth
}
