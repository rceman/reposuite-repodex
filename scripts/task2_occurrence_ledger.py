#!/usr/bin/env python3
"""TASK 2 occurrence-level audit ledger (v3).

Implements the *frozen* TASK 2 validation protocol (docs/TASK2_VALIDATION_PLAN.md
section 7) literally:

    a match requires the same category and the same source span (byte range)
    for the occurrence anchor
    for call-like occurrences the anchor is the whole expression range
    for declarations the anchor is the name range
    a prediction whose anchor is inside the expected span but not equal is
    counted as a match only when the region contains no other occurrence of the
    same category within that span; otherwise it is scored by its own span

Expected occurrences come from an independent *grammar-level* enumeration
(examples/grammar_enum.rs, which walks the pinned Tree-sitter tree directly and
never consults RepoDex's adapters or predictions). Test-evidence annotation is
re-derived in this script from the documented per-language contract
(docs/LANGUAGE_SPIKE.md) and from source text read directly, never from RepoDex
output. RepoDex predictions come from
`reposuite-repodex parse --json --include-facts`. The two sides are then matched
one-to-one by byte span.

Usage:
    python3 scripts/task2_occurrence_ledger.py \
        --repodex ./target/release/reposuite-repodex \
        --enum ./target/release/examples/grammar_enum \
        --corpus-root "$CORPUS_ROOT" \
        --out <ledger.jsonl> [--summary <summary.txt>]
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

# Frozen regions: selected from source properties only, never from RepoDex
# output, and frozen before any prediction was collected.
REGIONS = [
    dict(id="rust-primary", language="rust",
         rel="serde-rs_serde/test_suite/tests/test_borrow.rs", lines=(1, 196)),
    dict(id="rust-supplementary", language="rust",
         rel="tokio-rs_tokio/tokio/tests/sync_broadcast.rs", lines=(55, 132)),
    dict(id="go-primary", language="go",
         rel="spf13_cobra/args_test.go", lines=(1, 250)),
    dict(id="go-supplementary", language="go",
         rel="gin-gonic_gin/gin_integration_test.go", lines=(7, 26)),
    dict(id="python-primary", language="python",
         rel="django_django/tests/template_tests/test_loaders.py", lines=(1, 277)),
    dict(id="php-primary", language="php",
         rel="laravel_framework/tests/Integration/Console/PromptsAssertionTest.php",
         lines=(1, 424)),
]

# Grammar node kinds per language and category, taken from the documented
# extraction contract in docs/LANGUAGE_SPIKE.md (NOT from the adapters).
DECL_KINDS = {
    "rust": {"function_item", "struct_item", "enum_item", "union_item", "trait_item",
             "type_item", "const_item", "static_item", "field_declaration",
             "enum_variant"},
    "go": {"function_declaration", "method_declaration", "package_clause",
           "type_declaration", "var_declaration", "const_declaration"},
    "python": {"function_definition", "class_definition"},
    "php": {"class_declaration", "interface_declaration", "trait_declaration",
            "enum_declaration", "enum_case", "function_definition",
            "method_declaration", "property_declaration",
            "property_promotion_parameter", "namespace_definition",
            "const_declaration"},
}
IMPORT_KINDS = {
    "rust": {"use_declaration"},
    "go": {"import_declaration"},
    "python": {"import_statement", "import_from_statement"},
    "php": {"namespace_use_declaration"},
}
# Node kinds that carry an import *item* anchor per language.
IMPORT_ITEM_KINDS = {
    "rust": {"identifier", "scoped_identifier", "use_as_clause", "use_wildcard"},
    "go": {"import_spec"},
    "python": {"dotted_name", "aliased_import", "wildcard_import"},
    "php": {"namespace_use_clause"},
}
CALL_KINDS = {
    "rust": {"call_expression", "macro_invocation"},
    "go": {"call_expression"},
    "python": {"call"},
    "php": {"function_call_expression", "member_call_expression",
            "scoped_call_expression", "object_creation_expression"},
}
# Grammar node kinds that carry a test-declaration name, per the contract's
# documented name conventions.
TEST_DECL_KINDS = {
    "rust": {"function_item"},
    "go": {"function_declaration"},
    "python": {"function_definition", "class_definition"},
    "php": {"method_declaration", "function_definition"},
}

CATEGORIES = ["declarations", "import_statements", "import_items", "calls", "tests"]


def line_starts(data: bytes) -> list[int]:
    starts = [0]
    for i, b in enumerate(data):
        if b == 0x0A:
            starts.append(i + 1)
    return starts


def region_span(data: bytes, lines: tuple[int, int]) -> tuple[int, int]:
    starts = line_starts(data)
    first, last = lines
    start = starts[first - 1]
    end = starts[last] if last < len(starts) else len(data)
    return start, end


def run(cmd: list[str]) -> str:
    proc = subprocess.run(cmd, capture_output=True, text=True)
    return proc.stdout


def enum_nodes(enum_bin: str, path: Path) -> list[dict]:
    out = run([enum_bin, str(path)])
    nodes = []
    for line in out.splitlines():
        parts = line.split("\t")
        if len(parts) < 12:
            continue
        nodes.append(dict(
            kind=parts[0], start=int(parts[1]), end=int(parts[2]),
            row_start=int(parts[3]), col_start=int(parts[4]),
            row_end=int(parts[5]), col_end=int(parts[6]),
            parent=parts[7], field=parts[8],
            anchor_start=int(parts[9]), anchor_end=int(parts[10]),
            slice=parts[11],
        ))
    return nodes


def repodex_facts(repodex_bin: str, path: Path) -> dict:
    out = run([repodex_bin, "parse", str(path), "--json", "--include-facts"])
    return json.loads(out)["analysis"]


def in_span(node_start: int, node_end: int, span: tuple[int, int]) -> bool:
    return node_start >= span[0] and node_end <= span[1]


def rng(d: dict) -> tuple[int, int]:
    return (d["byte_start"], d["byte_end"])


def text(source: bytes, anchor: tuple[int, int]) -> str:
    return source[anchor[0]:anchor[1]].decode("utf-8", "replace")


# --- independent test-evidence annotation (from the documented contract) ---

def rust_is_test(n: dict, nodes: list[dict], source: bytes) -> bool:
    """A `function_item` preceded by `#[test]` / path-qualified `...::test`."""
    attrs = [a for a in nodes
             if a["kind"] == "attribute_item" and a["parent"] == n["parent"]
             and a["end"] <= n["start"]]
    if not attrs:
        return False
    attr = max(attrs, key=lambda a: a["end"])
    between = [d for d in nodes
               if d["kind"] in DECL_KINDS["rust"] and d["parent"] == n["parent"]
               and attr["end"] <= d["start"] < n["start"]]
    if between:
        return False
    seg = text(source, (attr["start"], attr["end"])).strip()
    seg = seg.lstrip("#![").rstrip("]").strip()
    last = seg.split("::")[-1].split("(")[0].strip()
    return last == "test"


def go_is_test(n: dict, nodes: list[dict], source: bytes) -> bool:
    """`Test`/`Benchmark`/`Fuzz` name prefix, or a single `*testing.T` param."""
    name = text(source, (n["anchor_start"], n["anchor_end"]))
    for prefix in ("Test", "Benchmark", "Fuzz"):
        if name.startswith(prefix):
            rest = name[len(prefix):]
            if rest and not rest[0].islower():
                return True
            break
    params = [x for x in nodes
              if x["parent"] == "function_declaration" and x["field"] == "parameters"
              and n["start"] <= x["start"] and x["end"] <= n["end"]]
    if params:
        ptext = text(source, (params[0]["start"], params[0]["end"])).strip()
        inner = ptext.strip("()").strip()
        for t in ("*testing.T", "*testing.B", "*testing.F"):
            if inner == f"{t}" or inner == f"t {t}":
                return True
    return False


def python_is_test(n: dict, nodes: list[dict], source: bytes) -> bool:
    """`test_` name prefix, a pytest mark decorator, or `TestCase` base."""
    name = text(source, (n["anchor_start"], n["anchor_end"]))
    if n["kind"] == "function_definition" and name.startswith("test_"):
        return True
    # decorators live in the enclosing `decorated_definition`
    decs = [d for d in nodes
            if d["kind"] == "decorator" and d["parent"] == "decorated_definition"
            and d["end"] <= n["start"]]
    if decs:
        dec = max(decs, key=lambda d: d["end"])
        dtext = text(source, (dec["start"], dec["end"]))
        if "pytest.mark" in dtext or "mark." in dtext:
            return True
    if n["kind"] == "class_definition":
        bases = [x for x in nodes
                 if x["parent"] == "class_definition" and x["field"] == "superclasses"
                 and n["start"] <= x["start"] and x["end"] <= n["end"]]
        if bases:
            btext = text(source, (bases[0]["start"], bases[0]["end"]))
            last = btext.rsplit(".", 1)[-1].strip()
            if last == "TestCase":
                return True
    return False


def php_is_test(n: dict, nodes: list[dict], source: bytes) -> bool:
    """`test` name prefix, or a `#[Test]` attribute."""
    name = text(source, (n["anchor_start"], n["anchor_end"]))
    if name.startswith("test"):
        return True
    attrs = [a for a in nodes
             if a["kind"] == "attribute_list" and a["parent"] == n["parent"]
             and a["end"] <= n["start"]]
    if attrs:
        attr = max(attrs, key=lambda a: a["end"])
        if "Test" in text(source, (attr["start"], attr["end"])):
            return True
    return False


TEST_PREDICATES = {
    "rust": rust_is_test,
    "go": go_is_test,
    "python": python_is_test,
    "php": php_is_test,
}


def rust_import_items(n: dict, nodes: list[dict]) -> list[tuple[int, int]]:
    """Item anchors for a Rust `use_declaration`.

    Braced lists expose items as `use_list` children; single paths expose the
    direct `scoped_identifier`/`identifier` argument.
    """
    items = [x for x in nodes
             if x["kind"] in IMPORT_ITEM_KINDS["rust"] and x["parent"] == "use_list"
             and n["start"] <= x["start"] and x["end"] <= n["end"]]
    if items:
        return [(x["start"], x["end"]) for x in items]
    direct = [x for x in nodes
              if x["kind"] in ("identifier", "scoped_identifier")
              and x["parent"] == "use_declaration"
              and n["start"] <= x["start"] and x["end"] <= n["end"]]
    return [(x["start"], x["end"]) for x in direct]


def import_items(language: str, n: dict, nodes: list[dict]) -> list[tuple[int, int]]:
    if language == "rust":
        return rust_import_items(n, nodes)
    if language == "go":
        specs = [x for x in nodes
                 if x["kind"] == "import_spec"
                 and n["start"] <= x["start"] and x["end"] <= n["end"]]
        return [(x["start"], x["end"]) for x in specs]
    if language == "python":
        if n["kind"] == "import_statement":
            return [(n["anchor_start"], n["anchor_end"])]
        items = [x for x in nodes
                 if x["kind"] in IMPORT_ITEM_KINDS["python"]
                 and x["parent"] == "import_from_statement"
                 and n["start"] <= x["start"] and x["end"] <= n["end"]
                 and x["field"] != "module_name"]
        return [(x["start"], x["end"]) for x in items]
    if language == "php":
        clauses = [x for x in nodes
                   if x["kind"] == "namespace_use_clause"
                   and n["start"] <= x["start"] and x["end"] <= n["end"]]
        return [(x["start"], x["end"]) for x in clauses]
    return []


_CALL_RE = re.compile(
    r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*|::[A-Za-z_][A-Za-z0-9_]*)*\(")


def rust_macro_argument_calls(n: dict, nodes: list[dict], source: bytes) -> list[tuple[int, int]]:
    """Source-written calls inside a Rust macro token tree (the F002 boundary).

    tree-sitter-rust presents macro arguments as a flat `token_tree`, so an inner
    call such as `tx.send("hello")` in `assert_ok!(tx.send("hello"))` is not a
    `call_expression` node. This enumerates it from source text, independently of
    RepoDex, so the boundary is scored rather than hidden.
    """
    out = []
    for tt in nodes:
        if tt["kind"] != "token_tree" or tt["parent"] != "macro_invocation":
            continue
        if not (n["start"] <= tt["start"] and tt["end"] <= n["end"]):
            continue
        raw = source[tt["start"]:tt["end"]].decode("utf-8", "replace")
        for m in _CALL_RE.finditer(raw):
            i = m.end() - 1  # index of the opening paren
            depth = 0
            j = i
            while j < len(raw):
                if raw[j] == "(":
                    depth += 1
                elif raw[j] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            out.append((tt["start"] + m.start(), tt["start"] + j + 1))
    return out


def decl_name_anchor(n: dict, nodes: list[dict]) -> tuple[int, int]:
    """The declaration's *name* byte anchor, from grammar structure only.

    The enumerator's default anchor is the `name` field child when present and
    otherwise the whole node. Two grammar shapes need refining so the anchor is
    the written name and not a sigil or the whole declaration:

    * Go `package_clause` -> the `package_identifier` child;
    * PHP `property_declaration` -> `property_element`(name) -> `variable_name`
      -> `name` (the identifier without the `$`), and any `name`-field child
      that is itself a `variable_name`.
    """
    kind = n["kind"]
    if kind == "package_clause":
        kids = [x for x in nodes if x["kind"] == "package_identifier"
                and n["start"] <= x["start"] and x["end"] <= n["end"]]
        if kids:
            return (kids[0]["start"], kids[0]["end"])
    if kind == "property_declaration":
        pe = [x for x in nodes if x["kind"] == "property_element"
              and n["start"] <= x["start"] and x["end"] <= n["end"]]
        if pe:
            vn = [x for x in nodes if x["kind"] == "variable_name" and x["field"] == "name"
                  and pe[0]["start"] <= x["start"] and x["end"] <= pe[0]["end"]]
            if vn:
                nm = [x for x in nodes if x["kind"] == "name"
                      and vn[0]["start"] <= x["start"] and x["end"] <= vn[0]["end"]]
                return (nm[0]["start"], nm[0]["end"]) if nm else (vn[0]["start"], vn[0]["end"])
    a = (n["anchor_start"], n["anchor_end"])
    if a == (n["start"], n["end"]):
        return a
    vn = [x for x in nodes if x["kind"] == "variable_name"
          and x["start"] == a[0] and x["end"] == a[1]]
    if vn:
        nm = [x for x in nodes if x["kind"] == "name" and a[0] <= x["start"] and x["end"] <= a[1]]
        if nm:
            return (nm[0]["start"], nm[0]["end"])
    return a


def expected_occurrences(language: str, nodes: list[dict], span: tuple[int, int],
                         source: bytes) -> list[dict]:
    expected = []
    predicate = TEST_PREDICATES[language]

    for n in nodes:
        if not in_span(n["start"], n["end"], span):
            continue
        kind = n["kind"]
        if kind in DECL_KINDS[language]:
            anchor = decl_name_anchor(n, nodes)
            expected.append(dict(category="declarations", expected_kind=kind,
                                 anchor=anchor, node_span=(n["start"], n["end"]),
                                 slice=n["slice"]))
            if kind in TEST_DECL_KINDS[language] and predicate(n, nodes, source):
                expected.append(dict(category="tests", expected_kind=kind,
                                     anchor=anchor, node_span=(n["start"], n["end"]),
                                     slice=n["slice"]))
        elif kind in IMPORT_KINDS[language]:
            expected.append(dict(category="import_statements", expected_kind=kind,
                                 anchor=(n["start"], n["end"]),
                                 node_span=(n["start"], n["end"]), slice=n["slice"]))
            for a in import_items(language, n, nodes):
                expected.append(dict(category="import_items", expected_kind=kind,
                                     anchor=a, node_span=(n["start"], n["end"]),
                                     slice=text(source, a)))
        elif kind in CALL_KINDS[language]:
            expected.append(dict(category="calls", expected_kind=kind,
                                 anchor=(n["start"], n["end"]),
                                 node_span=(n["start"], n["end"]), slice=n["slice"]))
            if language == "rust" and kind == "macro_invocation":
                for a in rust_macro_argument_calls(n, nodes, source):
                    expected.append(dict(category="calls",
                                         expected_kind="macro_argument_call",
                                         anchor=a, node_span=(n["start"], n["end"]),
                                         slice=text(source, a)))
    return expected


def predicted_occurrences(analysis: dict, span: tuple[int, int]) -> list[dict]:
    preds = []
    for d in analysis["declarations"]:
        r = rng(d["name_range"])
        if in_span(*rng(d["range"]), span):
            preds.append(dict(category="declarations", pred_kind=d["kind"],
                              anchor=r, value=d["name"], range=rng(d["range"])))
            if d.get("test_evidence"):
                preds.append(dict(category="tests", pred_kind=d["kind"],
                                  anchor=r, value=d["name"], range=rng(d["range"])))
    for imp in analysis["imports"]:
        if in_span(*rng(imp["statement_range"]), span):
            preds.append(dict(category="import_statements", pred_kind=imp["form"],
                              anchor=rng(imp["statement_range"]),
                              value=imp.get("module"), range=rng(imp["statement_range"])))
            for item in imp["items"]:
                preds.append(dict(category="import_items", pred_kind=item["category"],
                                  anchor=rng(item["range"]), value=item["target"],
                                  range=rng(item["range"])))
    for c in analysis["calls"]:
        if in_span(*rng(c["expression_range"]), span):
            preds.append(dict(category="calls", pred_kind=c["form"],
                              anchor=rng(c["expression_range"]),
                              value=c["callee_written"], range=rng(c["expression_range"])))
    return preds


def match(expected: list[dict], predicted: list[dict]) -> tuple[list[dict], list[dict], list[dict]]:
    """Frozen byte-span one-to-one matching. Returns (ledger_rows, fn, fp)."""
    used_pred = set()
    rows = []
    fn = []
    for exp in expected:
        cand = [i for i, p in enumerate(predicted)
                if i not in used_pred and p["category"] == exp["category"]
                and p["anchor"] == exp["anchor"]]
        if cand:
            i = cand[0]
            used_pred.add(i)
            rows.append(dict(adjudication="MATCHED_TP", **exp, prediction=predicted[i]))
            continue
        # tolerance rule: a prediction whose anchor lies strictly inside the
        # expected span, when the expected span contains no other prediction of
        # the same category
        inside = [i for i, p in enumerate(predicted)
                  if i not in used_pred and p["category"] == exp["category"]
                  and exp["anchor"][0] <= p["anchor"][0] and p["anchor"][1] <= exp["anchor"][1]]
        if len(inside) == 1:
            i = inside[0]
            used_pred.add(i)
            rows.append(dict(adjudication="MATCHED_TP", match_rule="anchor_inside_unique",
                             **exp, prediction=predicted[i]))
        else:
            fn.append(exp)
            rows.append(dict(adjudication="UNMATCHED_EXPECTED_FN", **exp, prediction=None))
    fp = []
    for i, p in enumerate(predicted):
        if i not in used_pred:
            fp.append(p)
            rows.append(dict(adjudication="UNMATCHED_PREDICTION_FP", **p))
    return rows, fn, fp


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repodex", required=True)
    ap.add_argument("--enum", required=True)
    ap.add_argument("--corpus-root", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--summary", default=None)
    args = ap.parse_args()

    root = Path(args.corpus_root)
    ledger_rows = []
    summary_lines = []
    totals = {c: dict(tp=0, fp=0, fn=0, n=0) for c in CATEGORIES}

    for region in REGIONS:
        path = root / region["rel"]
        data = path.read_bytes()
        span = region_span(data, region["lines"])
        nodes = enum_nodes(args.enum, path)
        analysis = repodex_facts(args.repodex, path)

        expected = expected_occurrences(region["language"], nodes, span, data)
        predicted = predicted_occurrences(analysis, span)
        rows, fn, fp = match(expected, predicted)

        for row in rows:
            row["region_id"] = region["id"]
            row["language"] = region["language"]
            row["relative_path"] = region["rel"]
            row["region_byte_span"] = list(span)
            ledger_rows.append(row)

        for cat in CATEGORIES:
            e = [r for r in rows if r["category"] == cat]
            tp = sum(1 for r in e if r["adjudication"] == "MATCHED_TP")
            f = sum(1 for r in e if r["adjudication"] == "UNMATCHED_EXPECTED_FN")
            p = sum(1 for r in e if r["adjudication"] == "UNMATCHED_PREDICTION_FP")
            totals[cat]["tp"] += tp
            totals[cat]["fn"] += f
            totals[cat]["fp"] += p
            totals[cat]["n"] += tp + f
        summary_lines.append(
            f"{region['id']:20s} {region['language']:7s} "
            f"expected={len(expected):4d} predicted={len(predicted):4d} "
            f"tp={sum(1 for r in rows if r['adjudication']=='MATCHED_TP'):4d} "
            f"fn={len(fn):3d} fp={len(fp):3d}")

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w") as fh:
        for row in ledger_rows:
            fh.write(json.dumps(row, sort_keys=True) + "\n")

    summary_lines.append("")
    summary_lines.append(f"{'category':18s} {'n':>5s} {'TP':>5s} {'FP':>5s} {'FN':>5s} "
                         f"{'precision':>10s} {'recall':>8s}")
    for cat in CATEGORIES:
        t = totals[cat]
        denom_p = t["tp"] + t["fp"]
        denom_r = t["tp"] + t["fn"]
        prec = f"{t['tp']/denom_p*100:.1f}%" if denom_p else "n/a"
        rec = f"{t['tp']/denom_r*100:.1f}%" if denom_r else "n/a"
        summary_lines.append(f"{cat:18s} {t['n']:5d} {t['tp']:5d} {t['fp']:5d} {t['fn']:5d} "
                             f"{prec:>10s} {rec:>8s}")

    text_out = "\n".join(summary_lines)
    print(text_out)
    if args.summary:
        Path(args.summary).write_text(text_out + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
