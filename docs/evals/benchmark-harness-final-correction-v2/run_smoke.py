#!/usr/bin/env python3
"""RepoDex benchmark smoke runner.

Reproduces the scored smoke campaign deterministically from the committed
corpus + gold contract + schedule. Executor mode `dangerous` scoped to
disposable benchmark worktrees.
"""
import argparse, json, subprocess, time, shutil, hashlib, sys
from pathlib import Path

import tracer, validator, gold

TIMEOUT = 420


def load(path):
    return json.loads(Path(path).read_text())


def rdx_packet(binary, state_dir, root, query):
    t0 = time.monotonic()
    out = subprocess.run([binary, "query", "--root", root, "--query", query,
                          "--state-dir", state_dir],
                         capture_output=True, text=True).stdout
    return out, (time.monotonic() - t0) * 1000


def prompt(task, arm, root, binary, state_dir):
    base = (f"Investigate this repository and complete the task, verifying with tools.\n\n"
            f"TASK:\n{task['task']}\n\n")
    pk = None
    if arm == "S1":
        pk, ms = rdx_packet(binary, state_dir, task["root"], task["task"])
        base += (f"A repository-analysis tool produced this evidence (RDX). "
                 f"Verify with tools.\n\nEVIDENCE:\n{pk}\n\n")
        return base, {"repodex_prepare_wall_ms": round(ms, 1), "packet_bytes": len(pk)}
    base += "Use read/grep/find_file_by_name/exec.\n"
    base += "\nReturn:\nANSWER:\n<answer>\nEVIDENCE:\n<path:line>\n"
    return base, {"repodex_prepare_wall_ms": 0, "packet_bytes": 0}


def setup_wt(task, workdir, sid):
    if task.get("mutating"):
        d = Path(workdir) / "wt" / sid
        if d.exists():
            shutil.rmtree(d)
        shutil.copytree(task["pristine_root"], d)
        return str(d)
    return task["root"]


def run_one(binary, state_dir, devin, tasks, task, arm, rep, out):
    sid = f"{task['id']}.{arm}.r{rep}"
    root = setup_wt(task, out, sid)
    ptxt, prep = prompt(task, arm, root, binary, state_dir)
    pf = Path(out) / "prompts" / f"{sid}.txt"
    pf.parent.mkdir(parents=True, exist_ok=True)
    pf.write_text(ptxt)
    exp = Path(out) / "exports" / f"{sid}.json"
    exp.parent.mkdir(parents=True, exist_ok=True)
    if exp.exists():
        exp.unlink()
    t0 = time.monotonic()
    try:
        subprocess.run([devin, "-p", "--prompt-file", str(pf), "--export", str(exp),
                        "--respect-workspace-trust", "false",
                        "--permission-mode", "dangerous"],
                       cwd=root, capture_output=True, timeout=TIMEOUT)
        timed_out = False
    except subprocess.TimeoutExpired:
        timed_out = True
    agent_wall = time.monotonic() - t0
    ok = exp.exists()
    d = json.loads(exp.read_text()) if ok else {}
    evs, summ = tracer.extract(d)
    answer = tracer.last_answer(d)
    rejected = summ["rejected"]
    v = validator.validate(task, answer, root, ok, rejected, timed_out)
    tools = summ["tools"]
    tr = {"session": sid, "task": task["id"], "arm": arm, "rep": rep,
          "executor_mode": "dangerous",
          "execution_status": v["execution_status"],
          "validation_status": v["validation_status"],
          "rejected": rejected,
          "repodex_prepare_wall_ms": prep["repodex_prepare_wall_ms"],
          "packet_bytes": prep["packet_bytes"],
          "agent_session_wall_ms": round(agent_wall * 1000),
          "combined_wall_ms": round(agent_wall * 1000 + prep["repodex_prepare_wall_ms"]),
          "final_metrics": d.get("final_metrics", {}),
          "model_calls": summ["model_calls"],
          "tool_categories": tools,
          "correct": v["validation_status"] == "TASK_SUCCESS",
          "validation": v, "events": evs}
    tp = Path(out) / "traces" / f"{sid}.json"
    tp.parent.mkdir(parents=True, exist_ok=True)
    tp.write_text(json.dumps(tr))
    return tr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--workdir", required=True)
    ap.add_argument("--corpus", default=str(Path(__file__).parent / "TASK_CORPUS.json"))
    ap.add_argument("--schedule", default=str(Path(__file__).parent / "SESSION_SCHEDULE.json"))
    ap.add_argument("--state-dir", default=None)
    ap.add_argument("--devin", default="devin")
    args = ap.parse_args()
    out = Path(args.workdir)
    for d in ("exports", "prompts", "traces", "wt"):
        (out / d).mkdir(parents=True, exist_ok=True)
    state_dir = args.state_dir or str(out / "state")
    tasks = {t["id"]: t for t in load(args.corpus)}
    sched = load(args.schedule)
    results = []
    for job in sched:
        t = tasks[job["task"]]
        tr = run_one(args.binary, state_dir, args.devin, tasks, t, job["arm"], job["rep"], out)
        results.append({"run": tr["session"], "execution_status": tr["execution_status"],
                        "validation_status": tr["validation_status"], "correct": tr["correct"]})
        print(tr["session"], tr["execution_status"], tr["validation_status"], flush=True)
    (out / "results.json").write_text(json.dumps(results, indent=1))
    inv = sum(1 for r in results if r["execution_status"] == "INFRA_INVALID")
    print(f"{len(results)} sessions, INFRA_INVALID={inv}")


if __name__ == "__main__":
    main()
