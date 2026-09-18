#!/usr/bin/env python3
"""TASK 3C independent call-candidate audit.

This script deliberately does **not** reuse the Rust candidate rule. It
re-derives the expected candidate set for every audited plain-name call with a
separate, simpler implementation of the documented lexical/module rule, and it
cross-checks every emitted candidate against the raw source bytes.

The ground truth is the bounded TASK 3C rule, not Rust name resolution. A
candidate is correct when it is exactly the set of source-written ``fn``
declarations the bounded rule can see; it is never checked against "the real
runtime target".

Audit selection is **source-occurrence driven**: the regions are chosen from
the normalized call facts and frozen before scoring, so the audit never only
checks the records RepoDex happened to emit.

For every audited call the script classifies:

    CORRECT_NONE            no candidate expected, none emitted
    CORRECT_SINGLE          exactly one expected candidate, exactly that one emitted
    CORRECT_MULTIPLE        the expected multi-candidate set was emitted
    FALSE_CANDIDATE         a candidate that is not an eligible `fn` with this name
    WRONG_SCOPE_CANDIDATE   an eligible-name `fn` from outside the bounded scope
    MISSING_CANDIDATE       an expected candidate the artifact omitted
    OUT_OF_SCOPE_CORRECT    a non-plain-name call correctly recorded out of scope

It also reports ``NOT_AVAILABLE_TO_TASK3C``: an approximate count of
call-shaped tokens inside macro token trees, which TASK 1 never extracted.

Usage:

    scripts/task3c_audit.py --snapshot <snap> --links <links> --candidates <dir> \
        --repository <repo> [--all] [--json out.json]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

# ---------------------------------------------------------------------------
# Loading
# ---------------------------------------------------------------------------


def load_snapshot(snapshot_dir: Path):
    manifest = json.loads((snapshot_dir / "manifest.json").read_text())
    analyses = {}
    for entry in manifest["files"]:
        if entry["language"] != "rust":
            continue
        path = snapshot_dir / "files" / f"{entry['object_key']}.json"
        analyses[entry["relative_path"]] = json.loads(path.read_text())
    return manifest, analyses


def load_candidates(candidates_dir: Path):
    manifest = json.loads((candidates_dir / "manifest.json").read_text())
    records = [
        json.loads(line)
        for line in (candidates_dir / "call_candidates.jsonl").read_text().splitlines()
        if line.strip()
    ]
    by_source = {}
    for record in records:
        by_source[(record["source"]["relative_path"], record["source"]["fact_id"])] = record
    return manifest, records, by_source


# ---------------------------------------------------------------------------
# Independent candidate derivation
# ---------------------------------------------------------------------------


def is_identifier(written: str) -> bool:
    name = written[2:] if written.startswith("r#") else written
    if not name:
        return False
    if not (name[0].isalpha() or name[0] == "_"):
        return False
    return all(c.isalnum() or c == "_" for c in name)


def in_scope(call) -> bool:
    """Whether a call is inside the bounded TASK 3C rule."""
    return (
        call["form"] == "plain_name"
        and not call["dynamic_callee"]
        and is_identifier(call["callee_written"])
    )


def enclosing_module_chain(scopes_by_id, start_id):
    """Scope chain from the call's scope up to and including the innermost
    enclosing module boundary (`module` or `file`), innermost first.

    This is an independent re-implementation of the bounded lexical rule. The
    chain stops at the first module boundary, so the search can never borrow a
    declaration from a parent or sibling module.
    """
    chain = []
    scope_id = start_id
    guard = 0
    while True:
        scope = scopes_by_id.get(scope_id)
        if scope is None:
            break
        chain.append(scope)
        if scope["kind"] in ("file", "module"):
            break
        parent = scope["parent_scope_id"]
        if parent is None:
            break
        scope_id = parent
        guard += 1
        if guard > len(scopes_by_id):
            break
    return chain


def expected_candidates(analysis, call):
    """The candidate set the bounded rule produces, derived independently.

    Returns the list of ``(relative_path, declaration_id)`` for the first
    lexical level that contains a matching ``function`` declaration, or the
    empty list when no level has one.
    """
    scopes_by_id = {s["scope_id"]: s for s in analysis["scopes"]}
    chain = enclosing_module_chain(scopes_by_id, call["scope_id"])
    name = call["callee_written"]
    for scope in chain:
        matches = [
            d
            for d in analysis["declarations"]
            if d["scope_id"] == scope["scope_id"]
            and d["kind"] == "function"
            and d["name"] == name
        ]
        if matches:
            return [(d["relative_path"], d["declaration_id"]) for d in matches]
    return []


def candidate_scope_ids(analysis):
    """Map declaration_id -> scope_id for eligible functions, for scope checks."""
    return {d["declaration_id"]: d["scope_id"] for d in analysis["declarations"]}


# ---------------------------------------------------------------------------
# Source cross-checks
# ---------------------------------------------------------------------------


def read_bytes(path: Path) -> bytes:
    try:
        return path.read_bytes()
    except OSError:
        return b""


def slice_text(source: bytes, byte_start: int, byte_end: int) -> str:
    """Slice source by *byte* offsets and decode. Tree-sitter ranges are byte
    offsets, so slicing a decoded string would misalign on multi-byte UTF-8."""
    return source[byte_start:byte_end].decode("utf-8", errors="replace")


MACRO_CALL = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(")


def count_macro_hidden_calls(analysis, source: bytes) -> int:
    """Approximate count of call-shaped tokens inside macro token trees.

    Macro invocations are recorded as ``macro_invocation`` calls whose
    ``expression_range`` covers the whole ``name!(...)``/``name!{...}`` form.
    Calls inside the token tree are never extracted (TASK 2 F002), so this is
    an upper bound on calls that were never available to TASK 3C.
    """
    total = 0
    for call in analysis["calls"]:
        if call["form"] != "macro_invocation":
            continue
        start = call["expression_range"]["byte_start"]
        end = call["expression_range"]["byte_end"]
        body = slice_text(source, start, end)
        # Drop the leading `name!` so the macro's own name is not counted.
        bang = body.find("!")
        inner = body[bang + 1 :] if bang >= 0 else body
        total += len(MACRO_CALL.findall(inner))
    return total


def declaration_name_text(declaration, source: bytes) -> str:
    rng = declaration["name_range"]
    return slice_text(source, rng["byte_start"], rng["byte_end"])


# ---------------------------------------------------------------------------
# Audit
# ---------------------------------------------------------------------------


def select_regions(analyses) -> list:
    """Deterministically pick audit regions independent of candidate output.

    The selection uses only the normalized structure: files with nested
    functions (lexical/shadowing coverage), files with inline modules (module
    boundary coverage), files with impl methods, and a stride sample of all
    in-scope files for breadth. The returned list is the frozen region set.
    """
    inscope = {}
    nested = []
    inmod = []
    impl = []
    plain = []
    for path, analysis in analyses.items():
        calls = [c for c in analysis["calls"] if in_scope(c)]
        if not calls:
            continue
        inscope[path] = len(calls)
        scopes = analysis["scopes"]
        by_id = {s["scope_id"]: s for s in scopes}
        has_nested_fn = any(
            s["kind"] == "function"
            and by_id.get(s["parent_scope_id"], {}).get("kind") in ("function", "closure")
            for s in scopes
        )
        has_inline_mod = any(
            s["kind"] == "module"
            and by_id.get(s["parent_scope_id"], {}).get("kind") in ("file", "module")
            for s in scopes
        )
        has_impl = any(s["kind"] == "impl" for s in scopes)
        if has_nested_fn:
            nested.append(path)
        elif has_inline_mod:
            inmod.append(path)
        elif has_impl:
            impl.append(path)
        else:
            plain.append(path)

    chosen = set()
    # Depth: the in-scope-richest files of each structural category.
    for group in (nested, inmod, impl):
        for path in sorted(group, key=lambda p: (-inscope[p], p))[:6]:
            chosen.add(path)
    # Breadth: a deterministic stride over every in-scope file.
    ordered = sorted(inscope)
    stride = max(1, len(ordered) // 25)
    for index, path in enumerate(ordered):
        if index % stride == 0:
            chosen.add(path)
    return sorted(chosen)


def audit(args) -> dict:
    snapshot_dir = Path(args.snapshot)
    candidates_dir = Path(args.candidates)
    repo = Path(args.repository)

    _snap_manifest, analyses = load_snapshot(snapshot_dir)
    cand_manifest, _records, by_source = load_candidates(candidates_dir)

    regions = select_regions(analyses)
    if args.all:
        regions = sorted(analyses)

    verdicts = Counter()
    false_candidates = []
    wrong_scope = []
    missing = []
    details = []
    macro_hidden = 0
    calls_in_scope = 0
    calls_out_of_scope = 0

    for path in regions:
        analysis = analyses[path]
        source = read_bytes(repo / path)
        macro_hidden += count_macro_hidden_calls(analysis, source)
        decl_scope = candidate_scope_ids(analysis)
        scopes_by_id = {s["scope_id"]: s for s in analysis["scopes"]}

        for call in analysis["calls"]:
            key = (path, call["call_id"])
            record = by_source.get(key)
            if not in_scope(call):
                calls_out_of_scope += 1
                if record and record["outcome"]["outcome"] == "out_of_scope":
                    verdicts["OUT_OF_SCOPE_CORRECT"] += 1
                elif record:
                    verdicts["OUT_OF_SCOPE_WRONG"] += 1
                    false_candidates.append((path, call["call_id"], call["callee_written"],
                                             "non-plain-name call produced a candidate outcome"))
                continue

            calls_in_scope += 1
            expected = expected_candidates(analysis, call)
            actual = []
            if record:
                actual = [
                    (c["relative_path"], c["declaration_id"])
                    for c in record["outcome"].get("candidates", [])
                ]
                if record["outcome"]["outcome"] == "single_candidate":
                    c = record["outcome"]["candidate"]
                    actual = [(c["relative_path"], c["declaration_id"])]

            expected_set = set(expected)
            actual_set = set(actual)

            # Source cross-check: every emitted candidate must be a `function`
            # declaration whose written name equals the callee.
            for cpath, cdec in actual_set:
                cand_analysis = analyses.get(cpath)
                if cand_analysis is None:
                    continue
                decl = next(
                    (d for d in cand_analysis["declarations"] if d["declaration_id"] == cdec),
                    None,
                )
                csrc = read_bytes(repo / cpath)
                if decl is None:
                    verdicts["FALSE_CANDIDATE"] += 1
                    false_candidates.append((cpath, cdec, "declaration id not in snapshot"))
                    continue
                if decl["kind"] != "function":
                    verdicts["FALSE_CANDIDATE"] += 1
                    false_candidates.append((cpath, cdec, f"candidate kind {decl['kind']} is not function"))
                    continue
                if declaration_name_text(decl, csrc) != call["callee_written"]:
                    verdicts["FALSE_CANDIDATE"] += 1
                    false_candidates.append(
                        (cpath, cdec, f"candidate name {declaration_name_text(decl, csrc)!r} != callee {call['callee_written']!r}")
                    )
                    continue
                if (cpath, cdec) not in expected_set:
                    # An eligible-name `fn`, but outside the bounded scope.
                    call_module = enclosing_module_chain(scopes_by_id, call["scope_id"])
                    chain_ids = {s["scope_id"] for s in call_module}
                    verdicts["WRONG_SCOPE_CANDIDATE"] += 1
                    wrong_scope.append(
                        (cpath, cdec, decl["name"],
                         f"decl scope {decl_scope.get(cdec)} not in call chain {sorted(chain_ids)}")
                    )

            for expected_cand in expected_set - actual_set:
                verdicts["MISSING_CANDIDATE"] += 1
                missing.append((expected_cand[0], expected_cand[1], call["callee_written"]))

            # Whole-record adjudication.
            if not expected_set and not actual_set:
                verdicts["CORRECT_NONE"] += 1
            elif actual_set == expected_set:
                if len(actual_set) == 1:
                    verdicts["CORRECT_SINGLE"] += 1
                else:
                    verdicts["CORRECT_MULTIPLE"] += 1
            else:
                # The candidate sets disagree; the specific FALSE_CANDIDATE,
                # WRONG_SCOPE_CANDIDATE or MISSING_CANDIDATE verdicts were
                # counted above.
                details.append(
                    (path, call["call_id"], call["callee_written"],
                     f"expected={sorted(expected_set)} actual={sorted(actual_set)}")
                )

    return {
        "regions": regions,
        "region_count": len(regions),
        "calls_in_scope": calls_in_scope,
        "calls_out_of_scope": calls_out_of_scope,
        "verdicts": dict(verdicts),
        "false_candidates": false_candidates,
        "wrong_scope": wrong_scope,
        "missing": missing,
        "divergences": details,
        "macro_hidden_calls": macro_hidden,
        "candidate_digest": cand_manifest["candidate_digest"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--snapshot", required=True)
    parser.add_argument("--links", required=True)
    parser.add_argument("--candidates", required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--all", action="store_true", help="audit every call, not just the frozen regions")
    parser.add_argument("--json", help="write the full report to a JSON file")
    args = parser.parse_args()

    report = audit(args)
    v = report["verdicts"]
    total = report["calls_in_scope"]
    correct = v.get("CORRECT_NONE", 0) + v.get("CORRECT_SINGLE", 0) + v.get("CORRECT_MULTIPLE", 0)
    tp = v.get("CORRECT_SINGLE", 0) + v.get("CORRECT_MULTIPLE", 0)
    fp = v.get("FALSE_CANDIDATE", 0) + v.get("WRONG_SCOPE_CANDIDATE", 0)
    fn = v.get("MISSING_CANDIDATE", 0)

    print("TASK 3C independent call-candidate audit")
    print(f"  regions:              {report['region_count']} files")
    print(f"  in-scope calls:       {total}")
    print(f"  out-of-scope calls:   {report['calls_out_of_scope']}")
    print(f"  correct none:         {v.get('CORRECT_NONE',0)}")
    print(f"  correct single:       {v.get('CORRECT_SINGLE',0)}")
    print(f"  correct multiple:     {v.get('CORRECT_MULTIPLE',0)}")
    print(f"  out-of-scope correct: {v.get('OUT_OF_SCOPE_CORRECT',0)}")
    print(f"  FALSE_CANDIDATE:      {v.get('FALSE_CANDIDATE',0)}")
    print(f"  WRONG_SCOPE:          {v.get('WRONG_SCOPE_CANDIDATE',0)}")
    print(f"  MISSING_CANDIDATE:    {v.get('MISSING_CANDIDATE',0)}")
    print(f"  OUT_OF_SCOPE_WRONG:   {v.get('OUT_OF_SCOPE_WRONG',0)}")
    print(f"  candidate TP/FP/FN:   {tp}/{fp}/{fn}")
    print(f"  macro-hidden calls:   {report['macro_hidden_calls']} (approx, NOT_AVAILABLE_TO_TASK3C)")

    if report["false_candidates"]:
        print("\n  FALSE_CANDIDATE detail:")
        for item in report["false_candidates"][:50]:
            print(f"    {item}")
    if report["wrong_scope"]:
        print("\n  WRONG_SCOPE detail:")
        for item in report["wrong_scope"][:50]:
            print(f"    {item}")
    if report["missing"]:
        print("\n  MISSING_CANDIDATE detail:")
        for item in report["missing"][:50]:
            print(f"    {item}")

    if args.json:
        Path(args.json).write_text(json.dumps(report, indent=2))
        print(f"\n  wrote {args.json}")

    # Fail loudly on any false or wrong-scope candidate.
    return 1 if (v.get("FALSE_CANDIDATE", 0) + v.get("WRONG_SCOPE_CANDIDATE", 0)) else 0


if __name__ == "__main__":
    sys.exit(main())
