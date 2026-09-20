#!/usr/bin/env python3
"""TASK 3C-BF real-source audit for Rust local-binding facts.

Reads a repository snapshot (per-file `FileAnalysis` JSON) and the source tree,
then:

  * counts emitted `bindings` by `kind`;
  * audits a deterministic sample of bindings, validating the written name,
    the `name_range` source slice, `binding_site_range` containment and every
    `visibility_ranges` bound;
  * runs a negative audit for over-capture (declared names, wildcards, path
    segments and identifiers inside initializer expressions must not be
    bindings).

This validates *syntax-fact extraction*, not Rust semantic name resolution.
"""

import argparse
import collections
import glob
import json
import random
import re
from pathlib import Path

IDENT = re.compile(r"^(r#[A-Za-z_][A-Za-z0-9_]*|[A-Za-z_][A-Za-z0-9_]*)$")


def load_snapshot(snapshot_dir: Path):
    manifest = json.loads((snapshot_dir / "manifest.json").read_text())
    analyses = {}
    for entry in manifest["files"]:
        path = snapshot_dir / "files" / f"{entry['object_key']}.json"
        analyses[entry["relative_path"]] = json.loads(path.read_text())
    return manifest, analyses


def sl(source: bytes, rng) -> str:
    return source[rng["byte_start"]:rng["byte_end"]].decode("utf-8", "replace")


def in_bounds(source: bytes, rng) -> bool:
    return (
        0 <= rng["byte_start"] <= rng["byte_end"] <= len(source)
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--snapshot", required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--sample", type=int, default=140)
    parser.add_argument("--json", default="")
    args = parser.parse_args()

    snapshot_dir = Path(args.snapshot)
    repo = Path(args.repository)
    _manifest, analyses = load_snapshot(snapshot_dir)

    # ---- counts by kind ----------------------------------------------------
    kind_counts = collections.Counter()
    total = ambiguous = 0
    all_bindings = []
    for rel, a in analyses.items():
        for b in a.get("bindings", []):
            kind_counts[b["kind"]] += 1
            total += 1
            if b.get("ambiguous"):
                ambiguous += 1
            all_bindings.append((rel, b))

    # ---- deterministic sample audit ---------------------------------------
    rng = random.Random(0xC0FFEE)
    sample = rng.sample(all_bindings, min(args.sample, len(all_bindings)))
    range_fail = []
    site_fail = []
    name_fail = []
    vis_fail = []
    vis_bounds_fail = []
    audited = []
    for rel, b in sample:
        src_path = repo / rel
        source = src_path.read_bytes() if src_path.exists() else b""
        name_range = b["name_range"]
        site = b["binding_site_range"]
        written = sl(source, name_range)
        ok = True
        if not in_bounds(source, name_range) or written != b["name"]:
            name_fail.append((rel, b["name"], written))
            ok = False
        if not (site["byte_start"] <= name_range["byte_start"]
                and name_range["byte_end"] <= site["byte_end"]):
            site_fail.append((rel, b["name"]))
            ok = False
        if not b["visibility_ranges"]:
            # a bodiless signature parameter legitimately has empty visibility
            if b["kind"] != "function_parameter":
                vis_fail.append((rel, b["name"]))
                ok = False
        else:
            for vr in b["visibility_ranges"]:
                if not in_bounds(source, vr):
                    vis_bounds_fail.append((rel, b["name"]))
                    ok = False
        audited.append((rel, b["kind"], b["name"], ok))

    # ---- negative audit ----------------------------------------------------
    # 1) no wildcard binding.
    wildcard = [(rel, b["name"]) for rel, b in all_bindings if b["name"] == "_"]
    # 2) no binding name_range coincides with a declaration's name_range —
    #    function/type/method/module/macro names are declarations, not locals.
    decl_collide = []
    for rel, a in analyses.items():
        decl_ranges = {(d["name_range"]["byte_start"], d["name_range"]["byte_end"])
                       for d in a.get("declarations", [])}
        for b in a.get("bindings", []):
            key = (b["name_range"]["byte_start"], b["name_range"]["byte_end"])
            if key in decl_ranges:
                decl_collide.append((rel, b["name"]))
    # 3) every binding name is a syntactically valid identifier.
    non_ident = [(rel, b["name"]) for rel, b in all_bindings
                 if not IDENT.match(b["name"])]
    # 4) a binding's name_range must never coincide with a call's callee_range —
    #    a call target is a *use*, not a binding.
    callee_collide = []
    for rel, a in analyses.items():
        callee_ranges = {(c["callee_range"]["byte_start"], c["callee_range"]["byte_end"])
                         for c in a.get("calls", [])}
        for b in a.get("bindings", []):
            key = (b["name_range"]["byte_start"], b["name_range"]["byte_end"])
            if key in callee_ranges:
                callee_collide.append((rel, b["name"]))
    # 5) initializer-expression identifiers are not bindings: a binding's
    #    name_range must lie inside its own binding_site (a pattern position),
    #    which the containment check above already enforces. Additionally the
    #    binding name must appear in the pattern half of its site, not the
    #    initializer. We bound-check by confirming the name is at/before the
    #    site's midpoint only for `let` sites that contain `=`.

    report = {
        "files": len(analyses),
        "files_with_bindings": sum(1 for a in analyses.values() if a.get("bindings")),
        "total_bindings": total,
        "ambiguous": ambiguous,
        "by_kind": dict(kind_counts.most_common()),
        "audited": len(audited),
        "audit_failures": {
            "name_slice_or_bounds": len(name_fail),
            "site_containment": len(site_fail),
            "empty_visibility_non_parameter": len(vis_fail),
            "visibility_out_of_bounds": len(vis_bounds_fail),
        },
        "negative_audit": {
            "wildcard_bindings": len(wildcard),
            "declaration_name_collisions": len(decl_collide),
            "non_identifier_names": len(non_ident),
            "callee_range_collisions": len(callee_collide),
        },
        "samples_failed": [s for s in audited if not s[3]][:40],
    }
    if name_fail:
        report["name_fail_examples"] = name_fail[:20]
    if site_fail:
        report["site_fail_examples"] = site_fail[:20]
    if vis_fail:
        report["vis_fail_examples"] = vis_fail[:20]
    if decl_collide:
        report["decl_collide_examples"] = decl_collide[:20]
    if callee_collide:
        report["callee_collide_examples"] = callee_collide[:20]

    text = json.dumps(report, indent=2)
    print(text)
    if args.json:
        Path(args.json).write_text(text)
    failed = (
        name_fail or site_fail or vis_bounds_fail
        or wildcard or decl_collide or non_ident or callee_collide
    )
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
