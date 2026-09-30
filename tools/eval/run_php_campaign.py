#!/usr/bin/env python3
"""PHP call-candidates campaign driver (PHP_CALL_CANDIDATES_V1 §56-§60).

3 arms x 32 tasks x 2 reps = 192 sessions (wave 1: first 16 tasks = 96):
  N0  native discovery only — no RepoDex available
  P1  baseline RepoDex binary (starting HEAD, no PHP candidate rules),
      repo_query -> `reposuite-repodex query --nav adaptive`
  P2  new RepoDex binary with the PHP candidate layer, same adaptive profile.

The Agent contract is identical for P1/P2; the only difference is the binary
the repo_query shim invokes. repo_query is benchmark scaffolding, classified
as a repodex call.
"""
import argparse, json, subprocess, time, shutil, hashlib, csv, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import tracer, validator, gold, metrics
from classify import classify_tool_ops

TIMEOUT = 420


def load(p): return json.loads(Path(p).read_text())


def fixture_digest_check(manifest):
    bad = []
    for f, want in manifest.items():
        p = Path(f)
        if not p.exists() or hashlib.sha256(p.read_bytes()).hexdigest() != want:
            bad.append(f)
    return bad


SHIM = """#!/bin/sh
# repo_query(question) -> reposuite-repodex query with externally bound root.
# F1: full RDX (default profile). A2: adaptive RDX. Chosen at shim-inject time.
exec {BIN} query --root "{ROOT}" --state-dir "{STATE}" {NAV} --query "$1" --direct
"""


def make_prompt(task, arm):
    base = (f"Investigate this repository and complete the task, verifying with tools.\n\n"
            f"TASK:\n{task['task']}\n\n")
    if arm == "N0":
        base += ("Use your file read / search / exec tools to inspect the repository.\n")
    else:
        base += (
            "A `repo_query` tool is available: run `./repo_query \"<question>\"` via exec "
            "to ask the RepoDex repository navigator about structure, symbols, "
            "callers/callees, relationships, paths, and manifests. It returns a "
            "bounded evidence packet.\n\n"
            "Use RepoDex first for repository discovery and structural navigation. "
            "Use exact source reads for verification. Use broad native repository "
            "search only after an explicit RepoDex gap, ambiguity, unsupported "
            "evidence, or insufficient evidence.\n")
    base += "\nReturn:\nANSWER:\n<answer>\nEVIDENCE:\n<path:line>\n"
    return base


def setup_wt(task, out, sid, arm, binaries, state_dir):
    # every session gets an isolated copy for mutation isolation + the shim
    d = Path(out) / "wt" / sid
    shutil.rmtree(d, ignore_errors=True)
    shutil.copytree(task["pristine_root"], d)
    if arm in ("P1", "P2"):
        binary = binaries[arm]
        shim = d / "repo_query"
        shim.write_text(SHIM.format(BIN=binary, ROOT=str(d), STATE=state_dir,
                                    NAV="--nav adaptive"))
        shim.chmod(0o755)
    return str(d)


def run_one(binaries, state_dir, devin, task, arm, rep, out):
    sid = f"{task['id']}.{arm}.r{rep}"
    root = setup_wt(task, out, sid, arm, binaries, state_dir)
    ptxt = make_prompt(task, arm)
    pf = Path(out) / "prompts" / f"{sid}.txt"; pf.parent.mkdir(parents=True, exist_ok=True); pf.write_text(ptxt)
    exp = Path(out) / "exports" / f"{sid}.json"; exp.parent.mkdir(parents=True, exist_ok=True)
    exp.unlink(missing_ok=True)
    t0 = time.monotonic(); timed_out = False
    try:
        subprocess.run([devin, "-p", "--prompt-file", str(pf), "--export", str(exp),
                        "--respect-workspace-trust", "false", "--permission-mode", "dangerous"],
                       cwd=root, capture_output=True, timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        timed_out = True
    agent_wall = (time.monotonic() - t0) * 1000
    ok = exp.exists()
    d = load(exp) if ok else {}
    evs, summ = tracer.extract(d)
    v = validator.validate(task, tracer.last_answer(d), root, ok, summ["rejected"], timed_out)
    tr = {"session": sid, "task": task["id"], "arm": arm, "rep": rep,
          "executor_mode": "dangerous",
          "execution_status": v["execution_status"], "validation_status": v["validation_status"],
          "rejected": summ["rejected"], "agent_session_wall_ms": round(agent_wall),
          "final_metrics": d.get("final_metrics", {}), "model_calls": summ["model_calls"],
          "tool_categories": summ["tools"], "correct": v["validation_status"] == "TASK_SUCCESS",
          "validation": v, "events": evs}
    tp = Path(out) / "traces" / f"{sid}.json"; tp.parent.mkdir(parents=True, exist_ok=True)
    tp.write_text(json.dumps(tr))
    return tr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True,
                    help="P2 binary (new implementation)")
    ap.add_argument("--baseline-binary", default=None,
                    help="P1 binary (starting HEAD baseline); defaults to --binary")
    ap.add_argument("--workdir", required=True)
    ap.add_argument("--corpus", required=True)
    ap.add_argument("--manifest", default=None)
    ap.add_argument("--state-dir", default=None)
    ap.add_argument("--devin", default="devin")
    ap.add_argument("--arms", default="N0,P1,P2")
    ap.add_argument("--reps", type=int, default=2)
    ap.add_argument("--limit", type=int, default=None)
    a = ap.parse_args()
    out = Path(a.workdir)
    for d in ("exports", "prompts", "traces", "wt"):
        (out / d).mkdir(parents=True, exist_ok=True)
    state_dir = a.state_dir or str(out / "state")
    tasks = load(a.corpus)
    proofs = [gold.prove(t, t["root"]) for t in tasks]
    gold_ok = all(p["all_proven"] for p in proofs)
    json.dump({"tasks": proofs, "FROZEN_GOLD_SOURCE_AUDIT_PASS": gold_ok},
              open(out / "GOLD_SOURCE_PROOFS.json", "w"), indent=1)
    if not gold_ok:
        print("BLOCKED: gold audit failed"); return 2
    if a.manifest:
        bad = fixture_digest_check(load(a.manifest))
        json.dump({"fixture_mismatch": bad}, open(out / "FIXTURE_CHECK.json", "w"))
        if bad:
            print(f"BLOCKED: fixture digest mismatch {bad}"); return 2
    if a.limit:
        tasks = tasks[: a.limit]
    binaries = {"P2": a.binary, "P1": a.baseline_binary or a.binary}
    results = []
    for t in tasks:
        for arm in a.arms.split(","):
            for rep in range(1, a.reps + 1):
                tr = run_one(binaries, state_dir, a.devin, t, arm, rep, out)
                results.append(tr)
                print(tr["session"], tr["execution_status"], tr["validation_status"], flush=True)
    with open(out / "SESSION_METRICS.csv", "w", newline="") as g:
        w = csv.writer(g); w.writerow(metrics.HEADER); [w.writerow(metrics.session_row(t)) for t in results]
    json.dump(results, open(out / "results.json", "w"), indent=1)
    inv = sum(1 for t in results if t["execution_status"] == "INFRA_INVALID")
    json.dump({"sessions": len(results), "INFRA_INVALID": inv,
               "SUCCESS": sum(1 for t in results if t["validation_status"] == "TASK_SUCCESS")},
              open(out / "SUMMARY.json", "w"), indent=1)
    print(f"{len(results)} sessions, INFRA_INVALID={inv}")


if __name__ == "__main__":
    raise SystemExit(main())
