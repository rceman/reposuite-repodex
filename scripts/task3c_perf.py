#!/usr/bin/env python3
"""TASK 3C performance and artifact-size validation.

Measures, on the Rust real corpus (tokio):

    candidate build wall time, with the phase breakdown the CLI reports
    candidate build peak RSS
    candidate verify/load time
    candidate artifact bytes, vs the TASK 3A snapshot and TASK 3B link bytes
    a repeated build, to prove canonical determinism
    a build into a different output directory, to prove location independence
    a build from a relocated checkout root, to prove root independence

and answers whether a full candidate rebuild is cheap enough that incremental
candidate mutation is unnecessary for now.

Usage:

    scripts/task3c_perf.py [--repository <repo>] [--json out.json]
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HOME = Path.home()
CORPORA = HOME / "reposuite" / "repodex" / "corpora"
REPO = Path(__file__).resolve().parent.parent
BINARY = REPO / "target" / "release" / "reposuite-repodex"


def run(args, measure=True):
    """Run a command, returning (stdout, wall seconds, peak RSS KB)."""
    if measure and Path("/usr/bin/time").exists():
        wrapper = ["/usr/bin/time", "-f", "@@%es@@%MKB"]
        started = time.monotonic()
        proc = subprocess.run(wrapper + args, capture_output=True, text=True, check=False)
        wall = time.monotonic() - started
        rss = 0
        match = re.search(r"@@([\d.]+)s@@(\d+)KB", proc.stderr)
        if match:
            wall = float(match.group(1))
            rss = int(match.group(2))
        return proc.stdout, wall, rss
    started = time.monotonic()
    proc = subprocess.run(args, capture_output=True, text=True, check=False)
    return proc.stdout, time.monotonic() - started, 0


def dir_bytes(path: Path) -> int:
    return sum(f.stat().st_size for f in path.rglob("*") if f.is_file())


def manifest_digest(candidates_dir: Path) -> str:
    return json.loads((candidates_dir / "manifest.json").read_text())["candidate_digest"]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", default=str(CORPORA / "tokio-rs_tokio"))
    parser.add_argument("--json")
    args = parser.parse_args()

    repo = Path(args.repository)
    work = Path(tempfile.mkdtemp(prefix="repodex-t3c-perf-"))
    try:
        results = {"repository": str(repo)}

        # 1. Snapshot + links (the upstream pipeline the candidate build needs).
        snap = work / "snap"
        links = work / "links"
        out, snap_wall, snap_rss = run([
            str(BINARY), "index", "build", str(repo), "--output", str(snap), "--no-gitignore"
        ])
        out, link_wall, link_rss = run([
            str(BINARY), "links", "build", str(snap), "--repository", str(repo), "--output", str(links)
        ])
        results["snapshot_build_s"] = round(snap_wall, 3)
        results["link_build_s"] = round(link_wall, 3)
        results["snapshot_bytes"] = dir_bytes(snap)
        results["link_bytes"] = dir_bytes(links)

        # 2. Candidate build with timing + RSS.
        candidates = work / "candidates"
        out, cand_wall, cand_rss = run([
            str(BINARY), "candidates", "build", str(snap), "--links", str(links),
            "--output", str(candidates), "--json",
        ])
        results["candidate_build_s"] = round(cand_wall, 3)
        results["candidate_build_rss_kb"] = cand_rss
        results["candidate_bytes"] = dir_bytes(candidates)
        build_json = json.loads(out)
        results["records"] = build_json["records"]
        results["cardinalities"] = build_json["cardinalities"]
        results["phases"] = build_json["phases"]
        results["size_ratio"] = round(results["candidate_bytes"] / results["snapshot_bytes"], 4)
        results["link_ratio"] = round(results["candidate_bytes"] / results["link_bytes"], 4)

        # 3. Candidate verify/load time.
        _, verify_wall, _ = run([
            str(BINARY), "candidates", "verify", str(candidates),
            "--snapshot", str(snap), "--links", str(links),
        ])
        results["candidate_verify_s"] = round(verify_wall, 3)

        # 4. Determinism: repeated build.
        candidates2 = work / "candidates2"
        run([
            str(BINARY), "candidates", "build", str(snap), "--links", str(links),
            "--output", str(candidates2),
        ])
        results["digest_a"] = manifest_digest(candidates)
        results["digest_repeat"] = manifest_digest(candidates2)
        results["repeat_identical"] = results["digest_a"] == results["digest_repeat"]

        # 5. Location independence: different output directory.
        candidates3 = work / "elsewhere" / "candidates"
        run([
            str(BINARY), "candidates", "build", str(snap), "--links", str(links),
            "--output", str(candidates3),
        ])
        results["digest_other_output"] = manifest_digest(candidates3)
        results["output_independent"] = results["digest_a"] == results["digest_other_output"]

        # 6. Root independence: relocate the checkout, rebuild the whole pipeline.
        root_b = work / "relocated"
        shutil.copytree(repo, root_b, symlinks=True)
        snap_b = work / "snap-b"
        links_b = work / "links-b"
        candidates_b = work / "candidates-b"
        run([str(BINARY), "index", "build", str(root_b), "--output", str(snap_b), "--no-gitignore"])
        run([str(BINARY), "links", "build", str(snap_b), "--repository", str(root_b), "--output", str(links_b)])
        run([str(BINARY), "candidates", "build", str(snap_b), "--links", str(links_b), "--output", str(candidates_b)])
        results["digest_other_root"] = manifest_digest(candidates_b)
        results["root_independent"] = results["digest_a"] == results["digest_other_root"]

        print("TASK 3C performance")
        for key, value in results.items():
            print(f"  {key:24} {value}")

        if args.json:
            Path(args.json).write_text(json.dumps(results, indent=2))
            print(f"  wrote {args.json}")
        return 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
