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

# TASK 3E: `qualified_path` calls are handled by the structural-path rule.
STRUCTURAL_PATH_RULE = "rust.call.structural_path_function_candidate"

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


def load_links(links_dir: Path):
    """Index persisted TASK 3B `use_path` relationships by the exact import
    occurrence they describe — `(relative_path, import_id, item_index)`."""
    links = [
        json.loads(line)
        for line in (links_dir / "links.jsonl").read_text().splitlines()
        if line.strip()
    ]
    by_import = {}
    for link in links:
        if link["kind"] != "use_path" or link["source"]["fact_kind"] != "import":
            continue
        source = link["source"]
        by_import[(source["relative_path"], source["fact_id"], source.get("item_index"))] = link
    return by_import


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


def binding_applies(scopes_by_id, binding_scope, call_scope):
    """Whether a local binding in `binding_scope` reaches a call in `call_scope`.

    Independent re-implementation of the transparent-scope gate: a `let`/
    parameter/pattern binding is visible in its own scope and inside nested
    closures (which capture it), but not inside a nested `fn`/method/item body.
    """
    level = call_scope
    guard = 0
    while True:
        if level == binding_scope:
            return True
        scope = scopes_by_id.get(level)
        if scope is None or scope["kind"] != "closure":
            return False
        parent = scope["parent_scope_id"]
        if parent is None:
            return False
        level = parent
        guard += 1
        if guard > len(scopes_by_id):
            return False


def covers(binding, byte_offset):
    """Half-open membership test against `visibility_ranges`."""
    return any(
        r["byte_start"] <= byte_offset < r["byte_end"]
        for r in binding.get("visibility_ranges", [])
    )


def import_local_name(item):
    """The local name a `use` item binds: its alias, else the leaf `::` segment.

    Wildcards bind no single name. `use a::b::{self}` binds `b`.
    """
    if item.get("wildcard"):
        return None
    if item.get("alias"):
        return item["alias"]
    parts = item["target"].split("::")
    if parts and parts[-1] == "self":
        return parts[-2] if len(parts) >= 2 else None
    return parts[-1] if parts else None


def is_fn_target(target, analyses):
    """Whether a link target is an eligible `function` declaration, verified
    against the snapshot, not only the link's own `declaration_kind` claim."""
    if target["kind"] != "declaration":
        return False
    analysis = analyses.get(target["relative_path"])
    if analysis is None:
        return False
    decls = analysis["declarations"]
    did = target["declaration_id"]
    return did < len(decls) and decls[did]["kind"] == "function"


def import_expected(path, imp, item_index, links_by_import, analyses):
    """The expected outcome when a `use` import owns the call name, derived
    from the persisted TASK 3B `use_path` relationship for that exact item.

    Returns ``{"candidates": [...], "reason": str|None, "link_outcome": str|None}``.
    `reason` is the expected `no_candidate` reason when `candidates` is empty.
    """
    link = links_by_import.get((path, imp["import_id"], item_index))
    if link is None:
        # No persisted relationship: the import still owns the name.
        return {"candidates": [], "reason": "blocked_by_import_binding",
                "link_outcome": None, "link_id": None}
    outcome = link["outcome"]
    kind = outcome["outcome"]
    if kind == "exact":
        target = outcome["target"]
        if is_fn_target(target, analyses):
            return {"candidates": [(target["relative_path"], target["declaration_id"])],
                    "reason": None, "link_outcome": "exact", "link_id": link["link_id"]}
        return {"candidates": [], "reason": "import_target_not_eligible_function",
                "link_outcome": "exact", "link_id": link["link_id"]}
    if kind == "ambiguous":
        fns = [
            (t["relative_path"], t["declaration_id"])
            for t in outcome["candidates"]
            if is_fn_target(t, analyses)
        ]
        if fns:
            return {"candidates": fns, "reason": None,
                    "link_outcome": "ambiguous", "link_id": link["link_id"]}
        return {"candidates": [], "reason": "import_target_not_eligible_function",
                "link_outcome": "ambiguous", "link_id": link["link_id"]}
    if kind == "unresolved":
        return {"candidates": [], "reason": "import_structurally_unresolved",
                "link_outcome": "unresolved", "link_id": link["link_id"]}
    if kind == "out_of_scope":
        return {"candidates": [], "reason": "import_out_of_scope",
                "link_outcome": "out_of_scope", "link_id": link["link_id"]}
    return {"candidates": [], "reason": "blocked_by_import_binding",
            "link_outcome": kind, "link_id": link["link_id"]}


def expected_outcome(path, analysis, call, links_by_import, analyses):
    """The expected result under the import-aware lexical-blocker rule, derived
    independently of the production rule.

    Returns a tuple ``(kind, payload)`` where ``kind`` is one of:

    ``("blocked", blocker_kind)``   a covering same-name blocker suppresses the
                                    outer function; ``blocker_kind`` is one of
                                    ``local_binding`` / ``ambiguous_binding`` /
                                    ``constant``.
    ``("import", expectation)``     a `use` owns the name; ``expectation`` is the
                                    ``import_expected`` dict (candidates or a
                                    no_candidate reason plus link outcome).
    ``("candidates", [decls])``     the expected ``(path, declaration_id)`` set
                                    for the nearest level with a ``function``.
    """
    name = call["callee_written"]
    call_byte = call["callee_range"]["byte_start"]
    scopes_by_id = {s["scope_id"]: s for s in analysis["scopes"]}

    # A covering same-name local binding is the innermost name-bearing
    # construct: it shadows every outer function/constant/import.
    covering = [
        b
        for b in analysis.get("bindings", [])
        if b["name"] == name
        and covers(b, call_byte)
        and binding_applies(scopes_by_id, b["scope_id"], call["scope_id"])
    ]
    if covering:
        nearest = max(
            covering, key=lambda b: (b["name_range"]["byte_start"], b["binding_id"])
        )
        return ("blocked", "ambiguous_binding" if nearest["ambiguous"] else "local_binding")

    for scope in enclosing_module_chain(scopes_by_id, call["scope_id"]):
        level = scope["scope_id"]
        functions = [
            d
            for d in analysis["declarations"]
            if d["scope_id"] == level and d["kind"] == "function" and d["name"] == name
        ]
        if functions:
            return (
                "candidates",
                [(d["relative_path"], d["declaration_id"]) for d in functions],
            )
        if any(
            d["scope_id"] == level and d["kind"] == "constant" and d["name"] == name
            for d in analysis["declarations"]
        ):
            return ("blocked", "constant")
        # A same-name `use` item owns the name here: resolve its TASK 3B link.
        matched = next(
            (
                (imp, index)
                for imp in analysis["imports"]
                if imp["scope_id"] == level
                for index, item in enumerate(imp["items"])
                if import_local_name(item) == name
            ),
            None,
        )
        if matched is not None:
            imp, index = matched
            return ("import", import_expected(path, imp, index, links_by_import, analyses))
    return ("candidates", [])


# Map an expected blocker kind to the emitted `NoCandidate` reason string.
# `import` is no longer a `blocked` kind — it resolves through its TASK 3B link.
BLOCK_REASON = {
    "local_binding": "shadowed_by_local_binding",
    "ambiguous_binding": "blocked_by_ambiguous_local_binding",
    "constant": "blocked_by_local_constant",
}

# Verdict to emit for a correctly-classified block of each kind.
BLOCK_VERDICT = {
    "local_binding": "CORRECTLY_BLOCKED_LOCAL_BINDING",
    "ambiguous_binding": "CORRECTLY_BLOCKED_AMBIGUOUS_BINDING",
    "constant": "CORRECTLY_BLOCKED_CONSTANT",
}


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
    links_by_import = load_links(Path(args.links))

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
                # TASK 3E: `qualified_path` calls are now handled by the
                # structural-path rule, which emits its own candidate /
                # no_candidate / out_of_scope outcomes. They are validated by
                # `task3e_audit.py`, not by the plain-name out-of-scope check.
                if call["form"] == "qualified_path":
                    if record and record["rule_id"] == STRUCTURAL_PATH_RULE:
                        verdicts["STRUCTURAL_PATH_HANDLED"] += 1
                    else:
                        verdicts["STRUCTURAL_PATH_MISSING"] += 1
                        false_candidates.append((path, call["call_id"], call["callee_written"],
                                                 "qualified_path call missing structural-path record"))
                elif record and record["outcome"]["outcome"] == "out_of_scope":
                    verdicts["OUT_OF_SCOPE_CORRECT"] += 1
                elif record:
                    verdicts["OUT_OF_SCOPE_WRONG"] += 1
                    false_candidates.append((path, call["call_id"], call["callee_written"],
                                             "non-plain-name call produced a candidate outcome"))
                continue

            calls_in_scope += 1
            kind, payload = expected_outcome(path, analysis, call, links_by_import, analyses)
            expected = payload if kind == "candidates" else []
            actual = []
            actual_reason = None
            if record:
                outcome = record["outcome"]
                actual = [
                    (c["relative_path"], c["declaration_id"])
                    for c in outcome.get("candidates", [])
                ]
                if outcome["outcome"] == "single_candidate":
                    c = outcome["candidate"]
                    actual = [(c["relative_path"], c["declaration_id"])]
                actual_reason = outcome.get("reason")

            # A call an expected blocker should suppress: the record must be a
            # `no_candidate` carrying the matching blocker reason. An emitted
            # candidate here means a closer name-bearing fact lost precedence to
            # a candidate — a lexical-precedence error.
            if kind == "blocked":
                blocker_kind = payload
                expected_reason = BLOCK_REASON[blocker_kind]
                verdict = BLOCK_VERDICT[blocker_kind]
                if actual:
                    verdicts["LEXICAL_PRECEDENCE_ERROR"] += 1
                    false_candidates.append(
                        (path, call["call_id"], call["callee_written"],
                         f"candidate emitted despite a {blocker_kind} blocker")
                    )
                elif record and record["outcome"]["outcome"] == "no_candidate" \
                        and actual_reason == expected_reason:
                    verdicts[verdict] += 1
                elif record and record["outcome"]["outcome"] == "no_candidate":
                    verdicts["BLOCKED_WRONG_REASON"] += 1
                    details.append(
                        (path, call["call_id"], call["callee_written"],
                         f"expected {blocker_kind} block ({expected_reason}), got reason {actual_reason!r}")
                    )
                else:
                    verdicts["MISSING_CANDIDATE"] += 1
                    missing.append((path, call["call_id"], call["callee_written"]))
                continue

            # A `use` owns the call name: the expected outcome is derived from
            # the persisted TASK 3B `use_path` relationship, independently of
            # the production import-candidate path.
            if kind == "import":
                expectation = payload
                expected_cands = set(expectation["candidates"])
                expected_reason = expectation["reason"]
                actual_set = set(actual)
                # Source cross-check: every emitted candidate must name a
                # `function` declaration in the snapshot.
                for cpath, cdec in actual_set:
                    cand_analysis = analyses.get(cpath)
                    decl = None
                    if cand_analysis is not None and cdec < len(
                        cand_analysis["declarations"]
                    ):
                        decl = cand_analysis["declarations"][cdec]
                    if decl is None or decl["kind"] != "function":
                        verdicts["FALSE_IMPORTED_CANDIDATE"] += 1
                        false_candidates.append(
                            (path, call["call_id"], call["callee_written"],
                             f"imported candidate {cpath}#{cdec} is not a snapshot function")
                        )
                if expected_cands:
                    if actual_set == expected_cands:
                        if len(actual_set) == 1:
                            verdicts["CORRECT_SINGLE_IMPORTED"] += 1
                        else:
                            verdicts["CORRECT_MULTIPLE_IMPORTED"] += 1
                    elif not actual_set:
                        verdicts["MISSING_IMPORTED_CANDIDATE"] += 1
                        missing.append((path, call["call_id"], call["callee_written"]))
                    else:
                        verdicts["WRONG_IMPORTED_TARGET"] += 1
                        details.append(
                            (path, call["call_id"], call["callee_written"],
                             f"import expected={sorted(expected_cands)} actual={sorted(actual_set)}")
                        )
                else:
                    if actual_set:
                        verdicts["FALSE_IMPORTED_CANDIDATE"] += 1
                        false_candidates.append(
                            (path, call["call_id"], call["callee_written"],
                             "candidate emitted despite an unresolved/non-function import")
                        )
                    elif record and record["outcome"]["outcome"] == "no_candidate" \
                            and actual_reason == expected_reason:
                        verdicts["CORRECT_IMPORT_NO_CANDIDATE"] += 1
                        if expectation["link_outcome"] in (
                            "unresolved", "out_of_scope", "ambiguous",
                        ):
                            verdicts["UPSTREAM_LINK_LIMITATION"] += 1
                    elif record and record["outcome"]["outcome"] == "no_candidate":
                        verdicts["BLOCKED_WRONG_REASON"] += 1
                        details.append(
                            (path, call["call_id"], call["callee_written"],
                             f"expected import reason {expected_reason!r}, got {actual_reason!r}")
                        )
                    else:
                        verdicts["MISSING_IMPORTED_CANDIDATE"] += 1
                        missing.append((path, call["call_id"], call["callee_written"]))
                continue

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
    tp = v.get("CORRECT_SINGLE", 0) + v.get("CORRECT_MULTIPLE", 0) \
        + v.get("CORRECT_SINGLE_IMPORTED", 0) + v.get("CORRECT_MULTIPLE_IMPORTED", 0)
    fp = v.get("FALSE_CANDIDATE", 0) + v.get("WRONG_SCOPE_CANDIDATE", 0) \
        + v.get("FALSE_IMPORTED_CANDIDATE", 0) + v.get("WRONG_IMPORTED_TARGET", 0) \
        + v.get("LEXICAL_PRECEDENCE_ERROR", 0)
    fn = v.get("MISSING_CANDIDATE", 0) + v.get("MISSING_IMPORTED_CANDIDATE", 0)
    blocked_local = v.get("CORRECTLY_BLOCKED_LOCAL_BINDING", 0)
    blocked_ambig = v.get("CORRECTLY_BLOCKED_AMBIGUOUS_BINDING", 0)
    blocked_const = v.get("CORRECTLY_BLOCKED_CONSTANT", 0)
    import_no_candidate = v.get("CORRECT_IMPORT_NO_CANDIDATE", 0)
    import_single = v.get("CORRECT_SINGLE_IMPORTED", 0)
    import_multiple = v.get("CORRECT_MULTIPLE_IMPORTED", 0)
    upstream_limited = v.get("UPSTREAM_LINK_LIMITATION", 0)

    print("TASK 3D independent call-candidate audit (import-aware)")
    print(f"  regions:              {report['region_count']} files")
    print(f"  in-scope calls:       {total}")
    print(f"  out-of-scope calls:   {report['calls_out_of_scope']}")
    print(f"  correct none:         {v.get('CORRECT_NONE',0)}")
    print(f"  correct single:       {v.get('CORRECT_SINGLE',0)}")
    print(f"  correct multiple:     {v.get('CORRECT_MULTIPLE',0)}")
    print(f"  out-of-scope correct: {v.get('OUT_OF_SCOPE_CORRECT',0)}")
    print(f"  blocked local binding:{blocked_local}")
    print(f"  blocked ambiguous:    {blocked_ambig}")
    print(f"  blocked constant:     {blocked_const}")
    print("  --- import-aware ---")
    print(f"  import single:        {import_single}")
    print(f"  import multiple:      {import_multiple}")
    print(f"  import no-candidate:  {import_no_candidate}")
    print(f"  upstream-link-limited:{upstream_limited}")
    print(f"  FALSE_CANDIDATE:      {v.get('FALSE_CANDIDATE',0)}")
    print(f"  WRONG_SCOPE:          {v.get('WRONG_SCOPE_CANDIDATE',0)}")
    print(f"  MISSING_CANDIDATE:    {v.get('MISSING_CANDIDATE',0)}")
    print(f"  FALSE_IMPORTED:       {v.get('FALSE_IMPORTED_CANDIDATE',0)}")
    print(f"  WRONG_IMPORTED:       {v.get('WRONG_IMPORTED_TARGET',0)}")
    print(f"  MISSING_IMPORTED:     {v.get('MISSING_IMPORTED_CANDIDATE',0)}")
    print(f"  LEXICAL_PRECEDENCE:   {v.get('LEXICAL_PRECEDENCE_ERROR',0)}")
    print(f"  BLOCKED_WRONG_REASON: {v.get('BLOCKED_WRONG_REASON',0)}")
    print(f"  OUT_OF_SCOPE_WRONG:   {v.get('OUT_OF_SCOPE_WRONG',0)}")
    print(f"  STRUCTURAL_PATH handled/missing: {v.get('STRUCTURAL_PATH_HANDLED',0)}/{v.get('STRUCTURAL_PATH_MISSING',0)}")
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

    # Fail loudly on any false, wrong-scope, wrong-import or precedence
    # candidate — the §29 acceptance rule requires all of these to be zero.
    bad = (
        v.get("FALSE_CANDIDATE", 0)
        + v.get("WRONG_SCOPE_CANDIDATE", 0)
        + v.get("FALSE_IMPORTED_CANDIDATE", 0)
        + v.get("WRONG_IMPORTED_TARGET", 0)
        + v.get("LEXICAL_PRECEDENCE_ERROR", 0)
        + v.get("STRUCTURAL_PATH_MISSING", 0)
        + v.get("OUT_OF_SCOPE_WRONG", 0)
    )
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
