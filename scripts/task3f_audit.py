#!/usr/bin/env python3
"""TASK 3F crate/target topology audit.

Independently re-derives the Cargo target topology for a repository —
manifest discovery, target enumeration, module-tree membership — WITHOUT the
production topology code, then compares it to the `rust_crate_target` entities
in a link artifact.

    python3 scripts/task3f_audit.py \
        --repository <repo> --links <links-dir> --snapshot <snap> \
        [--sample N] [--json out.json]
"""

from __future__ import annotations

import argparse
import json
import sys
import tomllib
from collections import Counter, defaultdict
from pathlib import Path


# ---------------------------------------------------------------------------
# Independent Cargo model
# ---------------------------------------------------------------------------

def find_manifests(repo: Path):
    """Every Cargo.toml inside the repo, repo-relative, sorted."""
    out = []
    for p in sorted(repo.rglob("Cargo.toml")):
        if ".git" in p.parts or "target" in p.parts:
            continue
        out.append(p.relative_to(repo).as_posix())
    return out


def parse_manifest(repo: Path, rel: str):
    data = tomllib.loads((repo / rel).read_text())
    return {
        "path": rel,
        "dir": str(Path(rel).parent).replace("\\", "/") if "/" in rel or rel != "Cargo.toml" else "",
        "has_package": "package" in data,
        "name": data.get("package", {}).get("name"),
        "lib": data.get("lib"),
        "bin": data.get("bin", []),
        "test": data.get("test", []),
        "bench": data.get("bench", []),
        "example": data.get("example", []),
        "autobins": data.get("package", {}).get("autobins", True),
        "autotests": data.get("package", {}).get("autotests", True),
        "autobenches": data.get("package", {}).get("autobenches", True),
        "autoexamples": data.get("package", {}).get("autoexamples", True),
        "members": data.get("workspace", {}).get("members", []),
        "exclude": data.get("workspace", {}).get("exclude", []),
    }


def dname(p):
    return p.rsplit("/", 1)[0] if "/" in p else ""


def bname(p):
    return p.rsplit("/", 1)[-1]


def stem(p):
    return bname(p)[:-3] if bname(p).endswith(".rs") else bname(p)


def convention_files(pkg_dir, sub, indexed):
    base = f"{pkg_dir}/{sub}" if pkg_dir else sub
    out = []
    for f in indexed:
        if dname(f) == base:
            out.append(f)
        elif sub == "src/bin" and dname(dname(f)) == base and bname(f) == "main.rs":
            out.append(f)
    return sorted(out)


def package_targets(manifest, indexed):
    """Independent target enumeration: (kind, name, root_file, rule)."""
    d = manifest["dir"]
    name0 = manifest["name"] or (bname(d) if d else "")
    lib_name = name0.replace("-", "_")
    tgt = []

    def idx(rel):
        return rel in indexed

    # lib
    lib = manifest["lib"] or {}
    rel = f"{d}/{lib.get('path','src/lib.rs')}" if d else lib.get("path", "src/lib.rs")
    if "path" in lib or lib.get("name"):
        if idx(rel):
            tgt.append(("lib", lib.get("name") or lib_name, rel, "rust.target.lib.explicit_path"))
    elif idx(rel):
        tgt.append(("lib", lib_name, rel, "rust.target.lib.default_src_lib"))

    # bins
    seen = set()
    for b in manifest["bin"]:
        rel = f"{d}/{b['path']}" if b.get("path") else f"{d}/src/bin/{b.get('name','bin')}.rs"
        rel = rel.lstrip("/")
        if idx(rel) and rel not in seen:
            seen.add(rel)
            tgt.append(("bin", b.get("name") or "bin", rel, "rust.target.bin.explicit"))
    if manifest["autobins"]:
        main = f"{d}/src/main.rs" if d else "src/main.rs"
        if idx(main) and main not in seen:
            seen.add(main)
            tgt.append(("bin", bname(d) if d else name0, main, "rust.target.bin.default_src_main"))
        for f in convention_files(d, "src/bin", indexed):
            if f in seen:
                continue
            seen.add(f)
            nm = stem(f)
            if nm == "main":
                nm = bname(dname(f))
            tgt.append(("bin", nm, f, "rust.target.bin.default_src_bin"))

    # tests / benches / examples
    for kind, decls, auto, sub in [
        ("integration_test", manifest["test"], manifest["autotests"], "tests"),
        ("bench", manifest["bench"], manifest["autobenches"], "benches"),
        ("example", manifest["example"], manifest["autoexamples"], "examples"),
    ]:
        seen = set()
        for t in decls:
            rel = f"{d}/{t['path']}" if t.get("path") else f"{d}/{sub}/{t.get('name',kind)}.rs"
            rel = rel.lstrip("/")
            if idx(rel) and rel not in seen:
                seen.add(rel)
                tgt.append((kind, t.get("name") or kind, rel, f"rust.target.{kind}.explicit"))
        if auto:
            for f in convention_files(d, sub, indexed):
                if f in seen:
                    continue
                seen.add(f)
                tgt.append((kind, stem(f), f, f"rust.target.{kind}.default"))
    return tgt


# ---------------------------------------------------------------------------
# Independent module tree (mirrors the snapshot facts)
# ---------------------------------------------------------------------------

def moddir(f):
    d = dname(f)
    n = bname(f)
    return d if n in ("mod.rs", "lib.rs", "main.rs") else (f"{d}/{n[:-3]}" if d else n[:-3])


def build_tree(root, analyses):
    """Files reachable from a crate root via `mod`, tracking each module's
    child-module directory. A crate root's dir is its containing directory; an
    external `name.rs`/`name/mod.rs` module's dir is `moddir`; an inline
    `mod name {}`'s dir is `<parent_dir>/name`."""
    if root not in analyses:
        return None
    files = {root}
    visited = {root}
    # (file, scope_id, module_dir)
    frontier = [(root, 0, dname(root))]
    while frontier:
        f, sid, mdir = frontier.pop()
        rec = analyses[f]
        for decl in rec["declarations"]:
            if decl["scope_id"] != sid or decl["kind"] != "module":
                continue
            if decl.get("body_range") is None:
                cands = [f"{mdir}/{decl['name']}.rs", f"{mdir}/{decl['name']}/mod.rs"]
                present = [c for c in cands if c in analyses]
                if len(present) == 1 and present[0] not in visited:
                    visited.add(present[0])
                    files.add(present[0])
                    frontier.append((present[0], 0, moddir(present[0])))
            else:
                sc = next(
                    (
                        s
                        for s in rec["scopes"]
                        if s["kind"] == "module"
                        and s.get("name") == decl["name"]
                        and s["range"] == decl["range"]
                    ),
                    None,
                )
                if sc:
                    frontier.append((f, sc["scope_id"], f"{mdir}/{decl['name']}"))
    return files


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repository", required=True)
    ap.add_argument("--links", required=True)
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--sample", type=int, default=100)
    ap.add_argument("--json")
    args = ap.parse_args()
    repo = Path(args.repository)

    # Load snapshot file records.
    man = json.loads((Path(args.snapshot) / "manifest.json").read_text())
    analyses = {}
    for ent in man["files"]:
        rec = json.loads(
            (Path(args.snapshot) / "files" / f"{ent['object_key']}.json").read_text()
        )
        analyses[rec["file"]["relative_path"]] = rec
    indexed = {p for p, r in analyses.items() if r["file"]["language"] == "rust"}

    # Load actual rust_crate_target entities.
    actual_targets = {}
    actual_membership = defaultdict(set)
    for line in (Path(args.links) / "entities.jsonl").read_text().splitlines():
        e = json.loads(line)
        if e["structural_kind"] != "rust_crate_target":
            continue
        a = dict(x.split("=", 1) for x in e["assumptions"] if "=" in x)
        actual_targets[e["key"]] = (a["target_kind"], a["target_name"], a["root_file"])
        for f in e["files"]:
            actual_membership[f].add(e["key"])

    # Independently derive expected topology.
    manifests = [parse_manifest(repo, m) for m in find_manifests(repo)]
    expected_targets = {}  # target_id -> (kind,name,root)
    expected_membership = defaultdict(set)
    for m in manifests:
        if not m["has_package"]:
            continue
        for kind, name, root, rule in package_targets(m, indexed):
            tid = f"{m['dir']}::{kind}::{name}"
            expected_targets[tid] = (kind, name, root)
            tree = build_tree(root, analyses)
            if tree:
                for f in tree:
                    expected_membership[f].add(tid)

    counts = Counter()
    details = []
    # Targets
    for tid, (kind, name, root) in expected_targets.items():
        if tid in actual_targets:
            if actual_targets[tid] == (kind, name, root):
                counts["CORRECT_TARGET"] += 1
            else:
                counts["WRONG_CRATE_MEMBERSHIP"] += 1  # target metadata mismatch
                details.append(("WRONG_TARGET_META", tid, actual_targets[tid], (kind, name, root)))
        else:
            counts["MISSING_TARGET"] += 1
            details.append(("MISSING_TARGET", tid))
    for tid in actual_targets:
        if tid not in expected_targets:
            counts["FALSE_TARGET"] += 1
            details.append(("FALSE_TARGET", tid))
    # Membership
    all_files = set(expected_membership) | set(actual_membership)
    n = 0
    for f in sorted(all_files):
        exp = expected_membership.get(f, set())
        act = actual_membership.get(f, set())
        if exp == act:
            counts["CORRECT_MEMBERSHIP" if exp else "CORRECT_NO_KNOWN_CRATE"] += 1
        elif exp and not act:
            counts["MISSING_MEMBERSHIP"] += 1
            details.append(("MISSING_MEMBERSHIP", f, sorted(exp), sorted(act)))
        elif act and not exp:
            counts["WRONG_CRATE_MEMBERSHIP"] += 1
            details.append(("WRONG_CRATE_MEMBERSHIP", f, sorted(exp), sorted(act)))
        else:
            counts["WRONG_CRATE_MEMBERSHIP"] += 1
            details.append(("WRONG_CRATE_MEMBERSHIP", f, sorted(exp), sorted(act)))
        n += 1
        if n >= max(args.sample, len(all_files)):
            break

    print("\n=== TASK 3F crate/target topology audit ===")
    print(f"manifests: {len(manifests)}  packages: {sum(1 for m in manifests if m['has_package'])}")
    print(f"expected targets: {len(expected_targets)}  actual: {len(actual_targets)}")
    for k, v in counts.most_common():
        print(f"  {k:28} {v}")
    for d in details[:40]:
        print("   !", d)
    if args.json:
        Path(args.json).write_text(json.dumps({"counts": dict(counts), "details": details}, indent=2))
    bad = counts.get("FALSE_TARGET", 0) + counts.get("WRONG_CRATE_MEMBERSHIP", 0) + counts.get("MISSING_TARGET", 0) + counts.get("MISSING_MEMBERSHIP", 0)
    if bad:
        print(f"\nFAIL: {bad}")
        return 1
    print("\nPASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
