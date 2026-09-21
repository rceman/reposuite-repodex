#!/usr/bin/env python3
"""TASK 4A Go package/module topology audit.

Independently re-derives the expected package/module membership for every Go
file, WITHOUT calling production topology code. It reads `go.mod` files and Go
`package` clauses straight from source bytes (allowed for audit validation per
TASK 4A §4) and compares them to the persisted `go_module` /
`go_package_topology` entities.

    python3 scripts/task4a_audit.py \
        --snapshot <snap> --links <links> --repository <repo> [--json out.json]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

PACKAGE_RE = re.compile(rb"^\s*package\s+([A-Za-z_][A-Za-z0-9_]*)")
MODULE_RE = re.compile(r"^\s*module\s+(?:\"([^\"]+)\"|(\S+))")

SKIP_DIRS = {".git", "vendor", "node_modules"}


def find_go_mods(root: Path):
    """Walk for `go.mod` files; return {rel_dir: module_path}."""
    mods = {}
    for p in sorted(root.rglob("go.mod")):
        rel = p.relative_to(root)
        if any(part in SKIP_DIRS for part in rel.parts):
            continue
        rel_dir = str(rel.parent).replace("\\", "/")
        if rel_dir == ".":
            rel_dir = ""
        try:
            text = p.read_text(errors="replace")
        except OSError:
            mods[rel_dir] = None
            continue
        mp = None
        for line in text.splitlines():
            line = line.split("//", 1)[0].strip()
            m = MODULE_RE.match(line)
            if m:
                mp = m.group(1) or m.group(2)
                break
        mods[rel_dir] = mp
    return mods


def package_clause(path: Path):
    """First `package X` clause in a .go file's bytes."""
    try:
        data = path.read_bytes()
    except OSError:
        return None
    for line in data.split(b"\n"):
        m = PACKAGE_RE.match(line)
        if m:
            return m.group(1).decode("ascii", "replace")
    return None


def dirname(p):
    return p.rsplit("/", 1)[0] if "/" in p else ""


def module_for(directory: str, mods):
    """Nearest enclosing module root: longest root_dir prefix."""
    best = None
    for root_dir in mods:
        if root_dir == "" or directory == root_dir or directory.startswith(root_dir + "/"):
            if best is None or len(root_dir) > len(best):
                best = root_dir
    return best


def package_kind(name: str):
    if name == "main":
        return "command_main"
    if name.endswith("_test"):
        return "external_test"
    return "ordinary"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--links", required=True)
    ap.add_argument("--repository", required=True)
    ap.add_argument("--json")
    args = ap.parse_args()

    root = Path(args.repository)
    manifest = json.loads((Path(args.snapshot) / "manifest.json").read_text())
    go_files = [
        f["relative_path"] for f in manifest["files"] if f.get("language") == "go"
    ]

    # Production entities.
    prod_modules = {}   # module_id -> (module_path, manifest)
    prod_pkgs = {}      # package key -> (kind, module, import_path, files)
    file_to_pkg = {}
    for line in (Path(args.links) / "entities.jsonl").read_text().splitlines():
        e = json.loads(line)
        if e["structural_kind"] == "go_module":
            a = dict(x.split("=", 1) for x in e["assumptions"] if "=" in x)
            prod_modules[e["key"]] = (a.get("module_path"), a.get("manifest"))
        elif e["structural_kind"] == "go_package":
            a = dict(x.split("=", 1) for x in e["assumptions"] if "=" in x)
            # `go_package` key is `directory:name` (`:name` at the root).
            directory, _, name = e["key"].rpartition(":")
            prod_pkgs[(directory, name)] = (
                e["key"],
                a.get("package_kind"),
                a.get("module"),
                a.get("import_path"),
                sorted(e["files"]),
            )
            for f in e["files"]:
                file_to_pkg[f] = e["key"]

    # Independent expectation.
    mods = find_go_mods(root)  # dir -> module_path
    exp_modules = {d or "<root>": mp for d, mp in mods.items() if mp}
    # group files by (dir, package_name)
    dir_pkgs = defaultdict(set)   # dir -> {package_name}
    file_pkg = {}                 # file -> package_name
    file_role = {}
    for rel in go_files:
        name = package_clause(root / rel)
        file_pkg[rel] = name
        file_role[rel] = "test" if rel.endswith("_test.go") else "source"
        if name:
            dir_pkgs[dirname(rel)].add(name)

    counts = Counter()
    details = []

    # Module audit: every production module must match a discovered go.mod.
    for mid, mp in exp_modules.items():
        exp_path = "go.mod" if mid == "<root>" else f"{mid}/go.mod"
        if mid in prod_modules:
            pm, pman = prod_modules[mid]
            verdict = (
                "CORRECT_MODULE"
                if pm == mp and pman == exp_path
                else "WRONG_MODULE_MEMBERSHIP"
            )
        else:
            verdict = "MISSING_MODULE"
        counts[verdict] += 1
        details.append({"module": mid, "verdict": verdict, "expected": mp})
    for mid in prod_modules:
        if mid not in exp_modules:
            counts["FALSE_MODULE"] += 1
            details.append({"module": mid, "verdict": "FALSE_MODULE"})

    # Package audit: per (dir, name) and per file membership.
    for directory, names in sorted(dir_pkgs.items()):
        mod_dir = module_for(directory, mods)
        for name in sorted(names):
            kind = package_kind(name)
            found = prod_pkgs.get((directory, name))
            if found is None:
                counts["MISSING_PACKAGE"] += 1
                details.append({"dir": directory, "pkg": name, "verdict": "MISSING_PACKAGE"})
                continue
            key, pk, pm, ip, pfiles = found
            if pk != kind:
                counts["PACKAGE_KIND_ERROR"] += 1
                details.append({"pkg": key, "expected_kind": kind, "got": pk})
                continue
            # A malformed `go.mod` boundary means NoKnownModule — its subtree is
            # excluded from the parent module, not folded into it.
            exp_mod = (
                (mod_dir or "<root>")
                if mod_dir is not None and mods.get(mod_dir)
                else "none"
            )
            if (pm or "none") != exp_mod:
                counts["WRONG_MODULE_MEMBERSHIP"] += 1
                details.append({"pkg": key, "expected_module": exp_mod, "got": pm})
                continue
            # import path
            if kind == "external_test":
                exp_ip = "none"
            elif mod_dir is None:
                exp_ip = "none"
            else:
                mp = mods[mod_dir]
                rel = directory[len(mod_dir):].lstrip("/") if mod_dir else directory
                exp_ip = mp if not rel else f"{mp}/{rel}"
            if (ip or "none") != exp_ip:
                counts["WRONG_MODULE_MEMBERSHIP"] += 1
                details.append({"pkg": key, "expected_import": exp_ip, "got": ip})
                continue
            counts[
                "CORRECT_EXTERNAL_TEST" if kind == "external_test" else "CORRECT_PACKAGE"
            ] += 1

    # File membership audit.
    for rel in go_files:
        name = file_pkg[rel]
        if name is None:
            if rel not in file_to_pkg:
                counts["CORRECT_NO_KNOWN_MODULE"] += 1  # no package -> no membership
            else:
                counts["WRONG_PACKAGE_MEMBERSHIP"] += 1
            continue
        if rel in file_to_pkg:
            counts["CORRECT_MEMBERSHIP"] += 1
        else:
            counts["WRONG_PACKAGE_MEMBERSHIP"] += 1
            details.append({"file": rel, "pkg": name, "verdict": "missing membership"})

    print("\n=== TASK 4A Go package/module topology audit ===")
    print(f"go files audited: {len(go_files)}")
    print(f"go.mod discovered: {len(mods)} (modules {len(exp_modules)})")
    for k, v in counts.most_common():
        print(f"  {k:30} {v}")
    bad = {
        k: v
        for k, v in counts.items()
        if k
        not in (
            "CORRECT_MODULE",
            "CORRECT_PACKAGE",
            "CORRECT_MEMBERSHIP",
            "CORRECT_EXTERNAL_TEST",
            "CORRECT_NO_KNOWN_MODULE",
        )
    }
    for d in details[:40]:
        print("   !", d)
    if args.json:
        Path(args.json).write_text(
            json.dumps({"counts": dict(counts), "details": details}, indent=2)
        )
    if bad:
        print(f"\nFAIL: {bad}")
        return 1
    print("\nPASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
