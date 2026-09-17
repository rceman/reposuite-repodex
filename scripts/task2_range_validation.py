#!/usr/bin/env python3
"""TASK 2 source-range validation over real corpus files.

For every supported file in the selected sample this checks, for every range
the adapter emits (declaration name/whole/body ranges, import statement/module/
target ranges, reference ranges, call expression/callee/type-argument ranges):

  * 0 <= byte_start <= byte_end <= len(source_bytes)
  * row_start <= row_end, and byte_start lies on the recorded start row
  * the recorded (row_start, column_start) is the true UTF-8 byte position of
    byte_start in the source

It also records how many sampled files contain multibyte UTF-8 or CRLF line
endings, so those are known to be represented in the sample.

Usage:
    task2_range_validation.py --binary <repodex> --root <corpus-root> \
        --language <rust|go|python|php> [--limit 80]

Corpus roots are supplied externally; nothing machine-specific is committed.
"""

import argparse
import json
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
    """Return a list of human-readable problems for one file's ranges."""
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
        want = position_of(source, start)
        got = (r["row_start"], r["column_start"])
        if want != got:
            problems.append(
                f"range {start}..{end}: recorded row/col {got} but byte {start} is at {want}"
            )
    return problems


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
    facts = {"declarations": 0, "imports": 0, "references": 0, "calls": 0, "tests": 0}
    bad_files = 0
    for path in files:
        source = path.read_bytes()
        proc = subprocess.run(
            [args.binary, "parse", str(path), "--json", "--include-facts"],
            capture_output=True,
        )
        # `parse` exits non-zero for a file that recovers or is unsupported, but
        # still emits the JSON analysis, so the exit code is not a skip signal.
        try:
            payload = json.loads(proc.stdout)
        except json.JSONDecodeError:
            print(f"  no analysis emitted: {path}", file=sys.stderr)
            continue
        analysis = payload.get("analysis") or {}
        collected = []
        ranges_in(analysis, collected)
        problems = validate(source, collected)
        totals["files"] += 1
        totals["ranges"] += len(collected)
        totals["bad"] += len(problems)
        if payload.get("status") == "recovered":
            totals["recovered"] += 1
        if problems:
            bad_files += 1
            for problem in problems[:3]:
                print(f"  {path}: {problem}", file=sys.stderr)
        if any(b > 0x7F for b in source):
            totals["multibyte"] += 1
        if b"\r\n" in source:
            totals["crlf"] += 1
        for key in facts:
            facts[key] += len(analysis.get(key, []))

    print(
        f"{args.language:8} files={totals['files']:4} ranges={totals['ranges']:7} "
        f"bad_ranges={totals['bad']:4} bad_files={bad_files:4} "
        f"recovered={totals['recovered']:3} "
        f"multibyte_files={totals['multibyte']:3} crlf_files={totals['crlf']:3} "
        f"decl={facts['declarations']} imports={facts['imports']} "
        f"refs={facts['references']} calls={facts['calls']} tests={facts['tests']}"
    )


if __name__ == "__main__":
    main()
