//! TASK 4B independent Go local-binding audit.
//!
//! Re-derives the expected set of local-binding introductions from the Go
//! syntax tree using its OWN traversal (it never calls the production binding
//! extraction in `parser/go.rs`), then diffs that expectation against the
//! persisted `bindings` facts. Also validates every emitted binding's ranges
//! and audits over-capture (names that must NOT become bindings).
//!
//!   cargo run --release --example task4b_audit -- --snapshot <snap> --repository <repo>

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use repodex::model::LocalBindingOccurrence;
use repodex::repository::artifact::{read_file_analysis, read_manifest};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Expected {
    name: String,
    name_start: u32,
    kind: &'static str,
    // For the audit we compare name+site+kind; visibility is validated by the
    // fixture truth tables and the range-validity pass below.
}

struct Ctx<'a> {
    src: &'a [u8],
    expected: Vec<Expected>,
}

fn text(src: &[u8], n: Node) -> String {
    String::from_utf8_lossy(&src[n.start_byte()..n.end_byte()]).into_owned()
}

fn idents_of_list(list: Node<'_>) -> Vec<Node<'_>> {
    let mut c = list.walk();
    list.children(&mut c)
        .filter(|n| n.is_named() && n.kind() == "identifier")
        .collect()
}

fn has_colon_eq(n: Node) -> bool {
    let mut c = n.walk();
    let f = n.children(&mut c).any(|x| x.kind() == ":=");
    f
}

// A lexical block's declared names (for `:=` new-vs-redecl). `local` marks a
// function-body-derived scope — package-level decls are never bindings (§31).
struct Scope {
    declared: HashSet<String>,
    local: bool,
}

impl<'a> Ctx<'a> {
    fn add(&mut self, n: Node, kind: &'static str) {
        let name = text(self.src, n);
        if name == "_" {
            return;
        }
        self.expected.push(Expected {
            name,
            name_start: n.start_byte() as u32,
            kind,
        });
    }

    fn param_list(&mut self, list: Node, kind: &'static str, scope: &mut Scope) {
        let mut c = list.walk();
        for p in list.children(&mut c) {
            if !matches!(
                p.kind(),
                "parameter_declaration" | "variadic_parameter_declaration"
            ) {
                continue;
            }
            let mut ic = p.walk();
            for (i, ch) in p.children(&mut ic).enumerate() {
                if p.field_name_for_child(i as u32) == Some("name") && ch.kind() == "identifier" {
                    let nm = text(self.src, ch);
                    if nm != "_" {
                        self.add(ch, kind);
                        scope.declared.insert(nm);
                    }
                }
            }
        }
    }

    fn signature(
        &mut self,
        node: Node,
        body: Option<Node>,
        pk: &'static str,
        rk: &'static str,
        recv: Option<Node>,
    ) {
        let mut scope = Scope {
            declared: HashSet::new(),
            local: true,
        };
        // Generic type parameters are blockers (`T(v)` is a plain_name call).
        if let Some(tp) = node.child_by_field_name("type_parameters") {
            let mut c = tp.walk();
            for decl in tp.children(&mut c) {
                if decl.kind() == "type_parameter_declaration" {
                    if let Some(n) = decl.child_by_field_name("name") {
                        if n.kind() == "identifier" && text(self.src, n) != "_" {
                            self.add(n, "type_parameter");
                            scope.declared.insert(text(self.src, n));
                        }
                    }
                }
            }
        }
        if let Some(r) = recv {
            self.param_list(r, "method_receiver", &mut scope);
        }
        if let Some(p) = node.child_by_field_name("parameters") {
            self.param_list(p, pk, &mut scope);
        }
        if let Some(res) = node.child_by_field_name("result") {
            if res.kind() == "parameter_list" {
                self.param_list(res, rk, &mut scope);
            }
        }
        if let Some(b) = body {
            self.block(b, &mut scope);
        }
    }

    fn block(&mut self, block_node: Node, scope: &mut Scope) {
        let mut c = block_node.walk();
        for child in block_node.children(&mut c) {
            if child.kind() == "statement_list" {
                let mut ic = child.walk();
                for s in child.children(&mut ic) {
                    if s.is_named() {
                        self.stmt(s, scope);
                    }
                }
            }
        }
    }

    fn stmt(&mut self, n: Node, scope: &mut Scope) {
        match n.kind() {
            "block" => {
                let mut inner = Scope {
                    declared: HashSet::new(),
                    local: scope.local,
                };
                self.block(n, &mut inner);
            }
            "short_var_declaration" => {
                if scope.local {
                    if let Some(left) = n.child_by_field_name("left") {
                        for id in idents_of_list(left) {
                            let nm = text(self.src, id);
                            if nm != "_" && !scope.declared.contains(&nm) {
                                self.add(id, "short_variable");
                                scope.declared.insert(nm);
                            }
                        }
                    }
                }
                if let Some(r) = n.child_by_field_name("right") {
                    self.stmt(r, scope);
                }
            }
            "var_declaration" | "const_declaration" | "type_declaration" if scope.local => {
                let kind = match n.kind() {
                    "const_declaration" => "constant",
                    "type_declaration" => "local_type",
                    _ => "variable",
                };
                let mut c = n.walk();
                for child in n.children(&mut c) {
                    let specs: Vec<Node> = match child.kind() {
                        "var_spec" | "const_spec" | "type_spec" | "type_alias" => vec![child],
                        "var_spec_list" | "const_spec_list" | "type_spec_list" => {
                            let mut ic = child.walk();
                            child.children(&mut ic).filter(|s| s.is_named()).collect()
                        }
                        _ => vec![],
                    };
                    for spec in specs {
                        let mut sc = spec.walk();
                        for (i, ch) in spec.children(&mut sc).enumerate() {
                            if spec.field_name_for_child(i as u32) == Some("name")
                                && matches!(
                                    ch.kind(),
                                    "identifier" | "field_identifier" | "type_identifier"
                                )
                            {
                                let nm = text(self.src, ch);
                                if nm != "_" {
                                    self.add(ch, kind);
                                    scope.declared.insert(nm);
                                }
                            }
                        }
                        if let Some(v) = spec.child_by_field_name("value") {
                            self.stmt(v, scope);
                        }
                    }
                }
            }
            // Package-level `var`/`const`/`type`: not bindings (§31), but a
            // `func` literal in a value still owns its own bindings — descend.
            "var_declaration" | "const_declaration" | "type_declaration" => {
                let mut c = n.walk();
                for child in n.children(&mut c) {
                    if child.is_named() {
                        self.stmt(child, scope);
                    }
                }
            }
            "if_statement" => {
                if let Some(init) = n.child_by_field_name("initializer") {
                    self.simple_init(init);
                    if let Some(r) = init.child_by_field_name("right") {
                        self.stmt(r, scope);
                    }
                }
                if let Some(cond) = n.child_by_field_name("condition") {
                    self.stmt(cond, scope);
                }
                if let Some(cons) = n.child_by_field_name("consequence") {
                    let mut inner = Scope {
                        declared: HashSet::new(),
                        local: scope.local,
                    };
                    self.block(cons, &mut inner);
                }
                if let Some(alt) = n.child_by_field_name("alternative") {
                    self.stmt(alt, scope);
                }
            }
            "for_statement" => {
                let mut c = n.walk();
                for child in n.children(&mut c) {
                    match child.kind() {
                        "for_clause" => {
                            if let Some(init) = child.child_by_field_name("initializer") {
                                self.simple_init(init);
                                if let Some(r) = init.child_by_field_name("right") {
                                    self.stmt(r, scope);
                                }
                            }
                            for f in ["condition", "update"] {
                                if let Some(x) = child.child_by_field_name(f) {
                                    self.stmt(x, scope);
                                }
                            }
                        }
                        "range_clause" => {
                            if has_colon_eq(child) {
                                if let Some(left) = child.child_by_field_name("left") {
                                    for id in idents_of_list(left) {
                                        if text(self.src, id) != "_" {
                                            self.add(id, "range_variable");
                                        }
                                    }
                                }
                            }
                            if let Some(r) = child.child_by_field_name("right") {
                                self.stmt(r, scope);
                            }
                        }
                        _ => {
                            if child.is_named() && child.kind() != "block" {
                                self.stmt(child, scope);
                            }
                        }
                    }
                }
                if let Some(body) = n.child_by_field_name("body") {
                    let mut inner = Scope {
                        declared: HashSet::new(),
                        local: scope.local,
                    };
                    self.block(body, &mut inner);
                }
            }
            "expression_switch_statement" => {
                if let Some(init) = n.child_by_field_name("initializer") {
                    self.simple_init(init);
                }
                if let Some(v) = n.child_by_field_name("value") {
                    self.stmt(v, scope);
                }
                self.clause_bodies(n, scope);
            }
            "type_switch_statement" => {
                if let Some(init) = n.child_by_field_name("initializer") {
                    self.simple_init(init);
                }
                if has_colon_eq(n) {
                    if let Some(alias) = n.child_by_field_name("alias") {
                        for id in idents_of_list(alias) {
                            if text(self.src, id) != "_" {
                                self.add(id, "type_switch_variable");
                            }
                        }
                    }
                }
                self.clause_bodies(n, scope);
            }
            "select_statement" => {
                let mut c = n.walk();
                for case in n.children(&mut c) {
                    if !matches!(case.kind(), "communication_case" | "default_case") {
                        continue;
                    }
                    let mut stmt_list = None;
                    let mut comm = None;
                    let mut ic = case.walk();
                    for part in case.children(&mut ic) {
                        match part.kind() {
                            "statement_list" => stmt_list = Some(part),
                            "receive_statement" | "send_statement" => comm = Some(part),
                            _ => {}
                        }
                    }
                    if let Some(recv) = comm {
                        if recv.kind() == "receive_statement" && has_colon_eq(recv) {
                            if let Some(left) = recv.child_by_field_name("left") {
                                for id in idents_of_list(left) {
                                    if text(self.src, id) != "_" {
                                        self.add(id, "select_receive_variable");
                                    }
                                }
                            }
                        }
                        self.stmt(recv, scope);
                    }
                    if let Some(list) = stmt_list {
                        let mut inner = Scope {
                            declared: HashSet::new(),
                            local: true,
                        };
                        let mut sc = list.walk();
                        for s in list.children(&mut sc) {
                            if s.is_named() {
                                self.stmt(s, &mut inner);
                            }
                        }
                    }
                }
            }
            "func_literal" => {
                // Its own signature/body — handled via signature().
                let body = n.child_by_field_name("body");
                self.signature(
                    n,
                    body,
                    "function_literal_parameter",
                    "function_literal_result",
                    None,
                );
            }
            _ => {
                let mut c = n.walk();
                for child in n.children(&mut c) {
                    if child.is_named() {
                        self.stmt(child, scope);
                    }
                }
            }
        }
    }

    fn simple_init(&mut self, init: Node) {
        if init.kind() == "short_var_declaration" {
            if let Some(left) = init.child_by_field_name("left") {
                for id in idents_of_list(left) {
                    if text(self.src, id) != "_" {
                        self.add(id, "short_variable");
                    }
                }
            }
        }
    }

    fn clause_bodies(&mut self, switch: Node, _scope: &mut Scope) {
        let mut c = switch.walk();
        for case in switch.children(&mut c) {
            if !matches!(
                case.kind(),
                "expression_case" | "type_case" | "default_case"
            ) {
                continue;
            }
            let mut ic = case.walk();
            for part in case.children(&mut ic) {
                if part.kind() == "statement_list" {
                    let mut inner = Scope {
                        declared: HashSet::new(),
                        local: true,
                    };
                    let mut sc = part.walk();
                    for s in part.children(&mut sc) {
                        if s.is_named() {
                            self.stmt(s, &mut inner);
                        }
                    }
                }
            }
        }
    }
}

fn expected_for_file(src: &[u8], tree: &tree_sitter::Tree) -> Vec<Expected> {
    let mut ctx = Ctx {
        src,
        expected: vec![],
    };
    let root = tree.root_node();
    let mut c = root.walk();
    for child in root.children(&mut c) {
        match child.kind() {
            "function_declaration" => {
                let body = child.child_by_field_name("body");
                ctx.signature(child, body, "function_parameter", "function_result", None);
            }
            "method_declaration" => {
                let body = child.child_by_field_name("body");
                ctx.signature(
                    child,
                    body,
                    "function_parameter",
                    "function_result",
                    child.child_by_field_name("receiver"),
                );
            }
            // Top-level var/const/type may carry a func literal value.
            _ => {
                let mut sc = Scope {
                    declared: HashSet::new(),
                    local: false,
                };
                ctx.stmt(child, &mut sc);
            }
        }
    }
    ctx.expected
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str| -> String {
        args.windows(2)
            .find(|w| w[0] == k)
            .map(|w| w[1].clone())
            .unwrap_or_default()
    };
    let snap = PathBuf::from(get("--snapshot"));
    let repo = PathBuf::from(get("--repository"));
    let manifest = read_manifest(&snap).expect("manifest");

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .unwrap();

    let mut tp = 0u64;
    let mut fp = 0u64;
    let mut fn_ = 0u64;
    let mut range_errors = 0u64;
    let mut over_capture = 0u64;
    let mut audited = 0u64;
    let mut details = Vec::new();

    for f in &manifest.files {
        if f.language != "go" {
            continue;
        }
        let rel = &f.relative_path;
        let src = std::fs::read(repo.join(rel)).expect("read source");
        let tree = parser.parse(&src, None).expect("parse");
        let analysis = read_file_analysis(&snap, f).expect("analysis");

        // Independent expectation.
        let mut expected = expected_for_file(&src, &tree);
        expected.sort();

        // Production bindings keyed by (name, name_range.start).
        let prod: BTreeMap<(String, u32), &LocalBindingOccurrence> = analysis
            .bindings
            .iter()
            .map(|b| ((b.name.clone(), b.name_range.byte_start), b))
            .collect();
        let exp_set: BTreeSet<(String, u32, &str)> = expected
            .iter()
            .map(|e| (e.name.clone(), e.name_start, e.kind))
            .collect();
        let exp_names: BTreeSet<(String, u32)> = expected
            .iter()
            .map(|e| (e.name.clone(), e.name_start))
            .collect();

        // Range validity + over-capture for every emitted binding.
        for b in &analysis.bindings {
            audited += 1;
            // name_range slices exactly to the name.
            let sliced = &src[b.name_range.byte_start as usize..b.name_range.byte_end as usize];
            if sliced != b.name.as_bytes() {
                range_errors += 1;
                details.push(format!("{rel}: name_range mismatch for {}", b.name));
            }
            // visibility within source, half-open.
            for v in &b.visibility_ranges {
                if v.byte_end as usize > src.len() || v.byte_start > v.byte_end {
                    range_errors += 1;
                }
            }
            // `_` must never appear.
            if b.name == "_" {
                over_capture += 1;
                details.push(format!("{rel}: blank identifier bound"));
            }
            if exp_names.contains(&(b.name.clone(), b.name_range.byte_start)) {
                tp += 1;
            } else {
                fp += 1;
                details.push(format!(
                    "{rel}: FP binding {} @{} kind {}",
                    b.name,
                    b.name_range.byte_start,
                    b.kind.as_str()
                ));
            }
        }
        for e in &exp_set {
            let key = (e.0.clone(), e.1);
            match prod.get(&key) {
                Some(b) if b.kind.as_str() == e.2 => {}
                Some(_) => {
                    details.push(format!("{rel}: kind mismatch for {} @{}", e.0, e.1));
                }
                None => {
                    fn_ += 1;
                    details.push(format!("{rel}: FN missing {} @{} kind {}", e.0, e.1, e.2));
                }
            }
        }
    }

    println!("\n=== TASK 4B Go local-binding independent audit ===");
    println!("emitted bindings validated: {audited}");
    println!("TP (binding == expected intro):        {tp}");
    println!("FP (emitted, no expected intro):       {fp}");
    println!("FN (expected intro, nothing emitted):  {fn_}");
    println!("range/source errors:                   {range_errors}");
    println!("over-capture (`_`/invalid):            {over_capture}");
    for d in details.iter().take(40) {
        println!("   ! {d}");
    }
    if fp + fn_ + range_errors + over_capture > 0 {
        println!("\nFAIL");
        std::process::exit(1);
    }
    println!("\nPASS");
    let _ = Path::new("");
}
