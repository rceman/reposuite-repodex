#!/usr/bin/env python3
"""TASK 2 extraction-quality audit harness.

Frozen regions are declared in REGIONS. They were selected **before** scoring,
using only source properties (path patterns and source-content markers such as
test conventions), never RepoDex predictions.

This harness performs two independent measurements over each frozen region:

  * RepoDex measurement - runs the release binary and counts every prediction
    whose anchor lies fully inside the region (declarations, import statements,
    import items, call-like occurrences, declarations carrying test evidence).
  * source measurement - a per-language scanner that reads only the source bytes
    and counts call-like occurrences, never consulting RepoDex output.

The two call counts are reconciled by hand and reported in
docs/TASK2_FINAL_REPORT.md with the full TP/FP/FN, precision and recall table.
A Rust call written inside a macro argument is a documented policy boundary
(finding F002): the scanner counts it, RepoDex does not, and it is reported
separately as a strict false negative rather than hidden.

Usage:
    task2_extraction_audit.py --binary <repodex> --corpus-root <dir> [--json out]
"""

import argparse
import json
import re
import subprocess
from pathlib import Path

REGIONS = [
    ("serde-rs_serde/test_suite/tests/test_borrow.rs", 1, 196, "rust", "primary"),
    ("tokio-rs_tokio/tokio/tests/sync_broadcast.rs", 55, 132, "rust", "supplementary"),
    ("spf13_cobra/args_test.go", 1, 250, "go", "primary"),
    ("gin-gonic_gin/gin_integration_test.go", 7, 26, "go", "supplementary"),
    ("django_django/tests/template_tests/test_loaders.py", 1, 277, "python", "primary"),
    ("laravel_framework/tests/Integration/Console/PromptsAssertionTest.php", 1, 424, "php", "primary"),
]

NON_CALLS = {
    "rust": {"if", "else", "for", "while", "loop", "match", "return", "fn", "struct", "enum",
             "impl", "trait", "let", "in", "as", "where", "use", "mod", "pub", "const", "static",
             "type", "unsafe", "async", "await", "move", "dyn", "ref", "mut", "self", "Self",
             "crate", "super", "break", "continue", "yield", "Some", "None", "Ok", "Err",
             "macro_rules", "derive", "serde", "allow", "cfg"},
    "go": {"if", "for", "switch", "select", "func", "go", "defer", "return", "range", "case",
           "make", "new", "len", "cap", "append", "string", "int", "bool", "error", "map",
           "chan", "interface", "struct", "type", "var", "const", "package", "import"},
    "python": {"if", "elif", "for", "while", "with", "return", "def", "class", "lambda", "assert",
               "del", "raise", "yield", "not", "and", "or", "in", "is", "import", "from", "except",
               "as", "print"},
    "php": {"if", "elseif", "while", "for", "foreach", "switch", "catch", "function", "fn",
            "array", "list", "unset", "echo", "print", "return", "match", "clone", "exit", "die",
            "include", "include_once", "require", "require_once", "use", "namespace", "class",
            "extends", "implements", "instanceof", "declare", "global", "static", "abstract",
            "final", "public", "private", "protected", "try", "throw", "else", "do", "case",
            "default", "break", "continue", "yield", "new", "int", "string", "bool", "float",
            "void", "iterable", "object", "mixed", "never", "callable", "readonly", "enum",
            "trait", "interface"},
}

STR = {
    "rust": re.compile(r'"(?:[^"\\]|\\.)*"'),
    "go": re.compile(r'"(?:[^"\\]|\\.)*"|`[^`]*`'),
    "python": re.compile(r'"[^"]*"|\'[^\']*\''),
    "php": re.compile(r'"[^"]*"|\'[^\']*\''),
}


def strip(line, lang):
    line = STR[lang].sub('""', line)
    line = re.sub(r"//.*$", "", line)
    if lang == "php":
        line = re.sub(r"#(?!\[).*$", "", line)
    return line


def macro_positions(line):
    inside, stack = set(), []
    for i, ch in enumerate(line):
        if ch == "(":
            j = i - 1
            while j >= 0 and line[j] == " ":
                j -= 1
            stack.append(j >= 0 and line[j] == "!" and (i - j) <= 3)
        elif ch == ")":
            if stack:
                stack.pop()
        if any(stack):
            inside.add(i)
    return inside


def scan_calls(window, lo, lang):
    """Independent call-like occurrence count from source bytes only."""
    calls, nested = [], []
    for offset, raw in enumerate(window):
        row = lo + offset
        code = strip(raw, lang)
        if lang == "rust":
            if re.match(r"\s*(pub\s+)?(async\s+)?(unsafe\s+)?(extern\s+\"[^\"]*\"\s+)?fn\s", code):
                continue
            if re.match(r"\s*#\[", code):
                continue
            inside = macro_positions(raw)
            for m in re.finditer(r"\b([a-zA-Z_]\w*)!\s*[\(\[\{]", code):
                calls.append((row, m.group(1)))
            for m in re.finditer(r"(?<![!\w])([A-Za-z_]\w*(?:::\w+)*(?:\.\w+)*)\s*\(", code):
                base = m.group(1).split("::")[-1].split(".")[-1]
                if base in NON_CALLS["rust"]:
                    continue
                (nested if m.start() in inside else calls).append((row, base))
        elif lang == "go":
            if re.match(r"\s*func\s", code):
                continue
            for m in re.finditer(r"(?<![!\w])([A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*)\s*\(", code):
                name = m.group(1)
                if name.split(".")[-1] in NON_CALLS["go"]:
                    continue
                calls.append((row, name.split(".")[-1]))
        elif lang == "python":
            if re.match(r"\s*(def|class)\s", code):
                continue
            for m in re.finditer(r"([A-Za-z_][A-Za-z0-9_.]*)\s*\(", code):
                if m.group(1).split(".")[-1] in NON_CALLS["python"]:
                    continue
                calls.append((row, m.group(1)))
        else:
            code = re.sub(r"function\s+\w+\s*\(", "function(", code)
            for m in re.finditer(r"new\s+([A-Za-z_\\][\w\\]*)\s*\(", code):
                calls.append((row, "new:" + m.group(1).split("\\")[-1]))
            for m in re.finditer(r"(?:->|::)\s*([A-Za-z_]\w*)\s*\(", code):
                calls.append((row, m.group(1)))
            for m in re.finditer(r"(?<![>\w$:])([A-Za-z_]\w*)\s*\(", code):
                if m.group(1) in NON_CALLS["php"]:
                    continue
                calls.append((row, m.group(1)))
    return calls, nested


def inside(a, b, lo, hi):
    return a >= lo - 1 and b <= hi - 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--corpus-root", required=True)
    ap.add_argument("--json")
    args = ap.parse_args()
    root = Path(args.corpus_root)
    out = {}

    for rel, lo, hi, lang, kind in REGIONS:
        path = root / rel
        window = path.read_bytes().decode("utf8", "replace").split("\n")[lo - 1:hi]
        proc = subprocess.run([args.binary, "parse", str(path), "--json", "--include-facts"],
                              capture_output=True)
        analysis = json.loads(proc.stdout)["analysis"]

        rd_decls = [d for d in analysis["declarations"]
                    if inside(d["name_range"]["row_start"], d["name_range"]["row_end"], lo, hi)]
        rd_calls = [c for c in analysis["calls"]
                    if inside(c["expression_range"]["row_start"], c["expression_range"]["row_end"], lo, hi)]
        rd_imports = [i for i in analysis["imports"]
                      if inside(i["statement_range"]["row_start"], i["statement_range"]["row_end"], lo, hi)]
        rd_tests = [d for d in rd_decls if d.get("test_evidence")]

        scan, nested = scan_calls(window, lo, lang)

        entry = {
            "kind": kind, "path": rel, "lines": [lo, hi], "language": lang,
            "repodex": {
                "declarations": len(rd_decls),
                "import_statements": len(rd_imports),
                "import_items": sum(len(i["items"]) for i in rd_imports),
                "calls": len(rd_calls),
                "declarations_with_test_evidence": len(rd_tests),
            },
            "source_scan": {
                "calls": len(scan) + len(nested),
                "calls_outside_macro_args": len(scan),
                "calls_inside_macro_args": len(nested),
            },
        }
        out.setdefault(lang, []).append(entry)
        print(f"{lang:7} {rel.split('/')[-1]:34} L{lo}-{hi} {kind:12} "
              f"RD decl {len(rd_decls):3} imp {len(rd_imports)}/{sum(len(i['items']) for i in rd_imports):3} "
              f"calls {len(rd_calls):3} tests {len(rd_tests):3} | "
              f"scan calls {len(scan)} + {len(nested)} nested = {len(scan) + len(nested)}")

    if args.json:
        Path(args.json).write_text(json.dumps(out, indent=2))
        print("wrote", args.json)


if __name__ == "__main__":
    main()
