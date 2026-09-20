#!/usr/bin/env python3
"""TASK 3E structural qualified-path candidate audit.

Independently re-derives the expected candidate set for every Rust
``qualified_path`` call in a snapshot, WITHOUT calling production candidate
code. It rebuilds the crate module tree from persisted snapshot facts
(`mod`/`fn`/`struct` declarations, module scopes, `use` imports, local
bindings) and resolves each `crate`/`self`/`super`/relative path by hand, then
compares the emitted candidate record.

    python3 scripts/task3e_audit.py \
        --snapshot <snap> --candidates <dir> [--all] [--json out.json]
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path

RULE = "rust.call.structural_path_function_candidate"


def load_snapshot(snapshot_dir: Path):
    manifest = json.loads((snapshot_dir / "manifest.json").read_text())
    analyses = {}
    for entry in manifest["files"]:
        path = snapshot_dir / "files" / f"{entry['object_key']}.json"
        rec = json.loads(path.read_text())
        analyses[rec["file"]["relative_path"]] = rec
    return manifest, analyses


def load_candidates(candidates_dir: Path):
    records = []
    for line in (candidates_dir / "call_candidates.jsonl").read_text().splitlines():
        records.append(json.loads(line))
    return records


# ---------------------------------------------------------------------------
# Module-tree construction (independent re-derivation of Structure::build).
# ---------------------------------------------------------------------------

def dirname(path: str) -> str:
    return path.rsplit("/", 1)[0] if "/" in path else ""


def basename(path: str) -> str:
    return path.rsplit("/", 1)[-1]


def rust_module_dir(file: str) -> str:
    d, n = dirname(file), basename(file)
    if n in ("mod.rs", "lib.rs", "main.rs"):
        return d
    stem = n[:-3] if n.endswith(".rs") else n
    return f"{d}/{stem}" if d else stem


class Module:
    __slots__ = ("path", "file", "scope_id", "dir", "declarations", "children")

    def __init__(self, path, file, scope_id):
        self.path = path  # crate-relative, e.g. ["crate","a","b"]
        self.file = file
        self.scope_id = scope_id
        self.dir = ""  # set by visit_module
        self.declarations = []  # non-module decls
        self.children = {}  # name -> [module indices]


class Crate:
    def __init__(self, root_file):
        self.root_file = root_file
        self.modules = []
        self.files = set()


def build_crates(analyses):
    """Build crate module trees from `lib.rs`/`main.rs` roots."""
    by_path = analyses
    crates = []
    roots = sorted(
        p
        for p, r in analyses.items()
        if r["file"]["language"] == "rust" and basename(p) in ("lib.rs", "main.rs")
    )
    for root in roots:
        crate = Crate(root)
        crate.files.add(root)
        crate.modules.append(Module(["crate"], root, 0))
        visited = {root}
        visit_module(crate, 0, analyses[root], 0, by_path, visited)
        crates.append(crate)
    return crates


def visit_module(crate, idx, rec, scope_id, by_path, visited):
    module = crate.modules[idx]
    module_dir = module.dir if module.dir else rust_module_dir(module.file)
    module.dir = module_dir
    # declarations directly inside this module scope
    for decl in rec["declarations"]:
        if decl["scope_id"] != scope_id:
            continue
        if decl["kind"] == "module":
            name = decl["name"]
            if decl.get("body_range") is None:
                # external `mod name;`
                first = f"{module_dir}/{name}.rs" if module_dir else f"{name}.rs"
                second = (
                    f"{module_dir}/{name}/mod.rs" if module_dir else f"{name}/mod.rs"
                )
                present = [f for f in (first, second) if f in by_path]
                if len(present) != 1:
                    continue
                child_file = present[0]
                if child_file in visited:
                    continue
                visited.add(child_file)
                path = module.path + [name]
                crate.modules.append(Module(path, child_file, 0))
                crate.files.add(child_file)
                module.children.setdefault(name, []).append(len(crate.modules) - 1)
                visit_module(
                    crate, len(crate.modules) - 1, by_path[child_file], 0, by_path, visited
                )
            else:
                # inline `mod name { }` — module scope matching name+range
                scope = next(
                    (
                        s
                        for s in rec["scopes"]
                        if s["kind"] == "module"
                        and s.get("name") == name
                        and s["range"] == decl["range"]
                    ),
                    None,
                )
                if scope is None:
                    continue
                path = module.path + [name]
                crate.modules.append(Module(path, rec["file"]["relative_path"], scope["scope_id"]))
                crate.modules[-1].dir = join(module_dir, name)
                module.children.setdefault(name, []).append(len(crate.modules) - 1)
                visit_module(
                    crate, len(crate.modules) - 1, rec, scope["scope_id"], by_path, visited
                )
        else:
            module.declarations.append(decl)


def join(d, n):
    return f"{d}/{n}" if d else n


# ---------------------------------------------------------------------------
# Independent path resolution.
# ---------------------------------------------------------------------------

def containing_module_path(crate, rec, scope_id, file_own):
    base = file_own.get(rec["file"]["relative_path"])
    if base is None:
        return None
    inline = []
    level = scope_id
    scopes = {s["scope_id"]: s for s in rec["scopes"]}
    guard = 0
    while level in scopes:
        sc = scopes[level]
        if sc["kind"] == "file":
            break
        if sc["kind"] == "module" and sc.get("name"):
            inline.append(sc["name"])
        level = sc.get("parent_scope_id")
        if level is None:
            break
        guard += 1
        if guard > len(scopes):
            break
    inline.reverse()
    return base + inline


def local_binding_shadows(rec, call, name):
    """A `let`/param/const binding `name` that covers the call shadows it."""
    call_byte = call["callee_range"]["byte_start"]
    for b in rec["bindings"]:
        if b["name"] != name:
            continue
        covers = any(
            vr["byte_start"] <= call_byte < vr["byte_end"]
            for vr in b.get("visibility_ranges", [])
        )
        if not covers:
            continue
        # binding applies if call scope == binding scope or reachable through
        # closures only.
        level, applies, guard = call["scope_id"], False, 0
        scopes = {s["scope_id"]: s for s in rec["scopes"]}
        while True:
            if level == b["scope_id"]:
                applies = True
                break
            sc = scopes.get(level)
            if sc is None or sc["kind"] != "closure":
                break
            level = sc.get("parent_scope_id")
            if level is None:
                break
            guard += 1
            if guard > len(scopes):
                break
        if applies:
            return True
    return False


def import_binds_root(rec, call, name):
    """A `use` item binding `name` in a scope enclosing the call owns it."""
    scopes = {s["scope_id"]: s for s in rec["scopes"]}
    level, guard = call["scope_id"], 0
    while level in scopes:
        sc = scopes[level]
        for imp in rec["imports"]:
            if imp["scope_id"] != sc["scope_id"]:
                continue
            for item in imp["items"]:
                if import_local_name(item) == name:
                    return True
        if sc["kind"] in ("file", "module"):
            break
        level = sc.get("parent_scope_id")
        if level is None:
            break
        guard += 1
        if guard > len(scopes):
            break
    return False


def import_local_name(item):
    if item.get("wildcard"):
        return None
    if item.get("alias"):
        return item["alias"]
    leaf = item["target"].rsplit("::", 1)
    last = leaf[-1]
    if last == "self" and len(leaf) > 1:
        return leaf[-2]
    return last


def descend(crate, start, segments):
    current = start
    for seg in segments:
        nxt = []
        for idx in current:
            nxt.extend(crate.modules[idx].children.get(seg, []))
        if not nxt:
            return None
        current = sorted(set(nxt))
    return current


def expected_outcome(rec, call, crates):
    """Return (expected_outcome_str, expected_names, classification)."""
    if call["type_arguments"]:
        return ("out_of_scope", [], "turbofish")
    written = call["callee_written"]
    segments = written.split("::")
    final = segments[-1]
    path_segs = segments[:-1]
    if not path_segs:
        return ("out_of_scope", [], "no_module_segment")
    first = path_segs[0]

    owners = [c for c in crates if rec["file"]["relative_path"] in c.files]
    if not owners:
        return ("out_of_scope", [], "no_crate")

    expected = []
    root_proven = False
    descent_ok = False
    for crate in owners:
        file_own = {}
        for m in crate.modules:
            if m.file not in file_own or len(m.path) < len(file_own[m.file]):
                file_own[m.file] = m.path
        module_by_path = {tuple(m.path): i for i, m in enumerate(crate.modules)}
        cpath = containing_module_path(crate, rec, call["scope_id"], file_own)
        if cpath is None:
            continue
        containing = module_by_path.get(tuple(cpath))
        if containing is None:
            continue
        if first == "crate":
            start, rest = [0], path_segs[1:]
        elif first == "self":
            start, rest = [containing], path_segs[1:]
        elif first == "super":
            up = 0
            while up < len(path_segs) and path_segs[up] == "super":
                up += 1
            if len(cpath) - up < 1:
                return ("out_of_scope", [], "super_above_crate")
            anc = module_by_path.get(tuple(cpath[: len(cpath) - up]))
            if anc is None:
                continue
            start, rest = [anc], path_segs[up:]
        else:
            # relative module root — shadowed or unproven?
            if local_binding_shadows(rec, call, first) or import_binds_root(
                rec, call, first
            ):
                return ("no_candidate", [], "root_shadowed")
            children = crate.modules[containing].children.get(first)
            if not children:
                return ("out_of_scope", [], "external_or_unproven_root")
            start, rest = list(children), path_segs[1:]
            root_proven = True
        if first in ("crate", "self", "super"):
            root_proven = True
        res = descend(crate, start, rest)
        if res is None:
            continue
        descent_ok = True
        for midx in res:
            for decl in crate.modules[midx].declarations:
                if decl["kind"] == "function" and decl["name"] == final:
                    expected.append(decl["name"])
    if expected:
        uniq = sorted(set(expected))
        return (("single" if len(uniq) == 1 else "multiple"), uniq, "structural")
    if descent_ok:
        return ("no_candidate", [], "terminal_not_function")
    if root_proven:
        return ("no_candidate", [], "segment_not_module")
    return ("out_of_scope", [], "unresolved")


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--snapshot", required=True)
    p.add_argument("--candidates", required=True)
    p.add_argument("--all", action="store_true")
    p.add_argument("--json")
    args = p.parse_args()

    _, analyses = load_snapshot(Path(args.snapshot))
    records = load_candidates(Path(args.candidates))
    cand_by_call = {
        (r["source"]["relative_path"], r["source"]["fact_id"]): r
        for r in records
        if r["rule_id"] == RULE
    }

    crates = build_crates(analyses)
    counts = Counter()
    details = []
    for path, rec in sorted(analyses.items()):
        if rec["file"]["language"] != "rust":
            continue
        for call in rec["calls"]:
            if call["form"] != "qualified_path":
                continue
            exp_out, exp_names, cls = expected_outcome(rec, call, crates)
            written = call["callee_written"]
            key = (path, call["call_id"])
            actual = cand_by_call.get(key)
            actual_out = actual["outcome"]["outcome"] if actual else "MISSING"
            actual_names = []
            if actual:
                oc = actual["outcome"]
                if oc["outcome"] == "single_candidate":
                    actual_names = [oc["candidate"]["name"]]
                elif oc["outcome"] == "multiple_candidates":
                    actual_names = [c["name"] for c in oc["candidates"]]

            if exp_out == "single" and actual_out == "single_candidate":
                verdict = (
                    "CORRECT_SINGLE_STRUCTURAL_PATH"
                    if actual_names == exp_names
                    else "WRONG_STRUCTURAL_PATH_TARGET"
                )
            elif exp_out == "multiple" and actual_out == "multiple_candidates":
                verdict = (
                    "CORRECT_MULTIPLE_STRUCTURAL_PATH"
                    if sorted(actual_names) == exp_names
                    else "WRONG_STRUCTURAL_PATH_TARGET"
                )
            elif exp_out == "no_candidate" and actual_out == "no_candidate":
                verdict = "CORRECT_NO_CANDIDATE"
            elif exp_out == "out_of_scope" and actual_out == "out_of_scope":
                verdict = "CORRECT_OUT_OF_SCOPE"
            elif exp_out in ("single", "multiple") and actual_out not in (
                "single_candidate",
                "multiple_candidates",
            ):
                verdict = "MISSING_STRUCTURAL_PATH_CANDIDATE"
            elif exp_out in ("no_candidate", "out_of_scope") and actual_out in (
                "single_candidate",
                "multiple_candidates",
            ):
                verdict = (
                    "ROOT_SHADOWING_ERROR" if cls == "root_shadowed"
                    else "FALSE_STRUCTURAL_PATH_CANDIDATE"
                )
            else:
                verdict = "MISMATCH"
            counts[verdict] += 1
            details.append(
                {
                    "path": path,
                    "call_id": call["call_id"],
                    "written": written,
                    "expected": exp_out,
                    "expected_names": exp_names,
                    "actual": actual_out,
                    "actual_names": actual_names,
                    "verdict": verdict,
                }
            )

    print("\n=== TASK 3E structural qualified-path audit ===")
    print(f"qualified_path calls audited: {sum(counts.values())}")
    for k, v in counts.most_common():
        print(f"  {k:40} {v}")
    bad = {k: v for k, v in counts.items() if k not in (
        "CORRECT_SINGLE_STRUCTURAL_PATH", "CORRECT_MULTIPLE_STRUCTURAL_PATH",
        "CORRECT_NO_CANDIDATE", "CORRECT_OUT_OF_SCOPE")}
    for d in details:
        if d["verdict"] in bad and bad[d["verdict"]] > 0:
            print(f"    ! {d['verdict']}: {d['written']} @ {d['path']}#{d['call_id']} "
                  f"exp={d['expected_names']} act={d['actual_names']}")
    if args.json:
        Path(args.json).write_text(json.dumps({"counts": dict(counts), "details": details}, indent=2))
    if bad:
        print(f"\nFAIL: {bad}")
        return 1
    print("\nPASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
