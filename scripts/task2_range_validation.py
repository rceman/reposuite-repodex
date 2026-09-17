#!/usr/bin/env python3
"""TASK 2 source-range validation over real corpus files (strengthened, R2).

For every supported file in the selected sample this checks, for every range the
adapter emits (declaration name/whole/body ranges, import statement/module/target
ranges, reference ranges, call expression/callee/type-argument ranges):

  * 0 <= byte_start <= byte_end <= len(source_bytes)
  * row_start <= row_end
  * the recorded (row_start, column_start) is the true UTF-8 byte position of
    byte_start in the source
  * the recorded (row_end, column_end) is the true UTF-8 byte position of
    byte_end in the source

It then performs *source-exact anchor validation* on representative fact types,
asserting that the byte slice is the intended construct:

  * declaration  name_range slice == declaration.name
  * call         callee_range slice == call.callee_written (when not dynamic)
  * call         expression_range slice contains the callee slice
  * import       statement_range slice == import.statement_text
  * import       module_range slice == import.module (when present)
  * import item  target is a substring of the item range slice
  * test evidence slice == evidence detail (attribute) or == declaration name

Two distinct measurements are reported separately:

  * positionally valid ranges   (bounds + row/column consistency)
  * source-exact anchor checks  (slice equals the intended construct)

Invalid JSON, unreadable files or a missing analysis are explicit *uncheckable*
failures, not silent skips: the script exits non-zero when any positional,
source-exact or uncheckable failure is found, so an incomplete measurement can
never be mistaken for a pass.

Usage:
    task2_range_validation.py --binary <repodex> --root <corpus-root> \
        --language <rust|go|python|php> [--limit 80]

Corpus roots are supplied externally; nothing machine-specific is committed.
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

EXTENSIONS = {
    "rust": {".rs"},
    "go": {".go"},
    "python": {".py"},
    "php": {".php"},
}


def ranges_in(node, out):
    """Collect every dict that looks like a SourceRange, recursively."""
    if isinstance(node, dict):
        if "byte_start" in node and "byte_end" in node:
            out.append(node)
        for value in node.values():
            ranges_in(value, out)
    elif isinstance(node, list):
        for value in node:
            ranges_in(value, out)


def position_of(source, offset):
    """(row, column) of a byte offset, both 0-based, column in UTF-8 bytes."""
    row = source.count(b"\n", 0, offset)
    line_start = source.rfind(b"\n", 0, offset) + 1
    return row, offset - line_start


def validate(source, ranges):
    """Positional problems for one file's ranges."""
    problems = []
    size = len(source)
    for r in ranges:
        start, end = r["byte_start"], r["byte_end"]
        if not (0 <= start <= end <= size):
            problems.append(f"range {start}..{end} outside 0..{size}")
            continue
        if "row_start" not in r:
            continue
        if r["row_start"] > r["row_end"]:
            problems.append(f"range {start}..{end} rows {r['row_start']}>{r['row_end']}")
        want_start = position_of(source, start)
        got_start = (r["row_start"], r["column_start"])
        if want_start != got_start:
            problems.append(
                f"range {start}..{end}: recorded start row/col {got_start} but byte {start} is at {want_start}"
            )
        if "row_end" in r and "column_end" in r:
            want_end = position_of(source, end)
            got_end = (r["row_end"], r["column_end"])
            if want_end != got_end:
                problems.append(
                    f"range {start}..{end}: recorded end row/col {got_end} but byte {end} is at {want_end}"
                )
    return problems


def sl(source, r):
    return source[r["byte_start"]:r["byte_end"]].decode("utf-8", "replace")


def norm_item(s):
    """Strip quoting and PHP `function`/`const` import prefixes and `$` sigils."""
    s = s.strip().strip("\"'").strip()
    for kw in ("function ", "const "):
        if s.startswith(kw):
            s = s[len(kw):].strip()
    if " as " in s:
        s = s.split(" as ")[0].strip()
    if s.startswith("$"):
        s = s[1:]
    return s.strip()


def item_matches_target(slice_text, target):
    """The item range slice is consistent with the (possibly qualified) target.

    Rust records the written local name while the target carries the enclosing
    module path (`OsStr` vs `ffi::OsStr`); Go quotes the path; PHP prefixes
    `function`/`const`.
    """
    got = norm_item(slice_text)
    return got == target or target.endswith(got) or got.endswith(target)


def evidence_matches(kind, slice_text, name, detail):
    """Source-exact assertion per documented test-evidence kind."""
    if kind == "attribute":
        # the range is the whole attribute item; the detail is the attribute
        # path, so `#[tokio::test(flavor = 1)]` is checked as `#[tokio::test]`
        path_only = re.sub(r"\(.*\)", "", slice_text.strip())
        return path_only == detail.strip()
    if kind == "decorator":
        return slice_text.strip().startswith("@")
    if kind in ("function_name_convention", "class_name_convention"):
        return slice_text == name
    if kind == "class_base_syntax":
        return "TestCase" in slice_text
    if kind == "signature_shape":
        return slice_text.strip().startswith("(")
    if kind == "file_name_convention":
        return slice_text == "" or slice_text.endswith(".go") or slice_text.endswith(".py")
    return True


def source_exact_checks(source, analysis):
    """Return (checked, problems) for the source-exact anchor assertions."""
    checked = 0
    problems = []

    for d in analysis.get("declarations", []):
        nr = d.get("name_range")
        if not nr:
            continue
        checked += 1
        got = sl(source, nr)
        if got != d.get("name"):
            problems.append(
                f"declaration name_range {nr['byte_start']}..{nr['byte_end']} slice {got!r} != name {d.get('name')!r}"
            )

    for c in analysis.get("calls", []):
        cr = c.get("callee_range")
        if not cr:
            continue
        checked += 1
        got = sl(source, cr)
        if not c.get("dynamic_callee") and got != c.get("callee_written"):
            problems.append(
                f"call callee_range {cr['byte_start']}..{cr['byte_end']} slice {got!r} != callee {c.get('callee_written')!r}"
            )
        er = c.get("expression_range")
        if er:
            checked += 1
            expr = sl(source, er)
            if got and got not in expr:
                problems.append(
                    f"call expression_range {er['byte_start']}..{er['byte_end']} does not contain callee {got!r}"
                )

    for imp in analysis.get("imports", []):
        sr = imp.get("statement_range")
        if sr and imp.get("statement_text") is not None:
            checked += 1
            got = sl(source, sr)
            if got != imp["statement_text"]:
                problems.append(
                    f"import statement_range {sr['byte_start']}..{sr['byte_end']} slice {got!r} != statement_text {imp['statement_text']!r}"
                )
        mr = imp.get("module_range")
        if mr and imp.get("module") is not None:
            checked += 1
            got = sl(source, mr)
            if got != imp["module"]:
                problems.append(
                    f"import module_range {mr['byte_start']}..{mr['byte_end']} slice {got!r} != module {imp['module']!r}"
                )
        for item in imp.get("items", []):
            ir = item.get("range")
            if not ir or item.get("target") is None:
                continue
            checked += 1
            got = sl(source, ir)
            if not item_matches_target(got, item["target"]):
                problems.append(
                    f"import item range {ir['byte_start']}..{ir['byte_end']} slice {got!r} inconsistent with target {item['target']!r}"
                )

    for d in analysis.get("declarations", []):
        for ev in d.get("test_evidence", []):
            er = ev.get("range")
            if not er:
                continue
            checked += 1
            got = sl(source, er)
            if not evidence_matches(ev.get("kind"), got, d.get("name"), ev.get("detail") or ""):
                problems.append(
                    f"test evidence ({ev.get('kind')}) range {er['byte_start']}..{er['byte_end']} slice {got!r} inconsistent with detail {ev.get('detail')!r} / name {d.get('name')!r}"
                )

    return checked, problems


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--root", required=True)
    ap.add_argument("--language", required=True, choices=sorted(EXTENSIONS))
    ap.add_argument("--limit", type=int, default=80)
    args = ap.parse_args()

    exts = EXTENSIONS[args.language]
    files = sorted(
        p for p in Path(args.root).rglob("*") if p.suffix in exts and ".git" not in p.parts
    )[: args.limit]

    totals = {"files": 0, "ranges": 0, "bad": 0, "multibyte": 0, "crlf": 0, "recovered": 0}
    exact = {"checked": 0, "bad": 0}
    uncheckable = 0
    facts = {"declarations": 0, "imports": 0, "references": 0, "calls": 0, "tests": 0}
    bad_files = 0
    for path in files:
        try:
            source = path.read_bytes()
        except OSError as exc:
            uncheckable += 1
            print(f"  unreadable: {path}: {exc}", file=sys.stderr)
            continue
        proc = subprocess.run(
            [args.binary, "parse", str(path), "--json", "--include-facts"],
            capture_output=True,
        )
        try:
            payload = json.loads(proc.stdout)
        except json.JSONDecodeError:
            uncheckable += 1
            print(f"  no analysis emitted: {path}", file=sys.stderr)
            continue
        analysis = payload.get("analysis") or {}
        if not analysis:
            uncheckable += 1
            print(f"  empty analysis: {path}", file=sys.stderr)
            continue
        collected = []
        ranges_in(analysis, collected)
        problems = validate(source, collected)
        checked, exact_problems = source_exact_checks(source, analysis)
        totals["files"] += 1
        totals["ranges"] += len(collected)
        totals["bad"] += len(problems)
        exact["checked"] += checked
        exact["bad"] += len(exact_problems)
        if payload.get("status") == "recovered":
            totals["recovered"] += 1
        if problems or exact_problems:
            bad_files += 1
            for problem in (problems + exact_problems)[:3]:
                print(f"  {path}: {problem}", file=sys.stderr)
        if any(b > 0x7F for b in source):
            totals["multibyte"] += 1
        if b"\r\n" in source:
            totals["crlf"] += 1
        for key in facts:
            facts[key] += len(analysis.get(key, []))

    print(
        f"{args.language:8} files={totals['files']:4} ranges={totals['ranges']:7} "
        f"positional_bad={totals['bad']:4} source_exact_checked={exact['checked']:6} "
        f"source_exact_bad={exact['bad']:4} uncheckable={uncheckable:3} "
        f"bad_files={bad_files:4} recovered={totals['recovered']:3} "
        f"multibyte_files={totals['multibyte']:3} crlf_files={totals['crlf']:3} "
        f"decl={facts['declarations']} imports={facts['imports']} "
        f"refs={facts['references']} calls={facts['calls']} tests={facts['tests']}"
    )
    if totals["bad"] or exact["bad"] or uncheckable:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
