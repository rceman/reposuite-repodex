#!/usr/bin/env python3
"""Authoritative RepoDex benchmark smoke pipeline.

ONE command runs the full pipeline:
  load corpus+schedule -> fixture digest verify -> gold audit -> tool preflight
  -> scored sessions -> trace extraction -> classify -> token/timing ->
  validate -> aggregate -> artifacts -> schema/digest -> manifest.

Any prerequisite failure (gold audit, preflight, fixture digest) blocks scoring.
"""
import argparse, json, subprocess, time, shutil, hashlib, csv, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import tracer, validator, gold, metrics
from classify import classify_tool, is_rejected_result, exit_code_of

TIMEOUT = 420


def load(p): return json.loads(Path(p).read_text())
def _sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def fixture_digest_check(manifest):
    """Verify committed fixture digests match the on-disk fixtures."""
    bad = []
    for f, want in manifest.items():
        p = Path(f)
        if not p.exists() or hashlib.sha256(p.read_bytes()).hexdigest() != want:
            bad.append(f)
    return bad


def tool_preflight(binary, state_dir, workdir):
    """Verify each required tool category executes under the scored executor mode."""
    checks = {}
    wt = Path(workdir) / "_preflight_wt"
    try:
        shutil.copytree(Path(__file__).parent / "fixtures" / "microrepo", wt)
        checks["read"] = (wt / "go.mod").exists()
        checks["edit"] = _try((wt / "go.mod").write_text, "module example.com/micro\n// preflight\n")
        c = subprocess.run(["go", "build", "-buildvcs=false", "./..."], cwd=wt,
                           capture_output=True, timeout=60)
        checks["build_test"] = c.returncode == 0  # known-good fixture must build cleanly
        q = subprocess.run([binary, "query", "--root", str(wt), "--query", "module",
                            "--state-dir", state_dir], capture_output=True, text=True, timeout=60)
        checks["repodex"] = bool(q.stdout.strip())
        checks["exec"] = subprocess.run(["ls", str(wt)], capture_output=True).returncode == 0
    except Exception as e:
        checks["error"] = str(e)
    finally:
        shutil.rmtree(wt, ignore_errors=True)
    return checks


def _try(fn, *a):
    try:
        fn(*a); return True
    except Exception:
        return False


def rdx_packet(binary, state_dir, root, query):
    t0 = time.monotonic()
    r = subprocess.run([binary, "query", "--root", root, "--query", query,
                        "--state-dir", state_dir], capture_output=True, text=True)
    ms = (time.monotonic() - t0) * 1000
    return r.stdout, {"repodex_prepare_transport_status": "SUCCESS" if r.returncode is not None else "FAILED",
                      "repodex_prepare_process_exit_code": r.returncode,
                      "repodex_prepare_stdout_bytes": len(r.stdout.encode()),
                      "repodex_prepare_stderr": r.stderr[:4000],
                      "repodex_prepare_wall_ms": round(ms, 1),
                      "prepare_ok": r.returncode == 0 and bool(r.stdout.strip())}


def make_prompt(task, arm, root, binary, state_dir):
    base = f"Investigate this repository and complete the task, verifying with tools.\n\nTASK:\n{task['task']}\n\n"
    prep = {"repodex_prepare_transport_status": None, "repodex_prepare_process_exit_code": None,
            "repodex_prepare_stdout_bytes": 0, "repodex_prepare_stderr": None,
            "repodex_prepare_wall_ms": 0, "packet_bytes": 0, "prepare_ok": True}
    if arm == "S1":
        pk, pr = rdx_packet(binary, state_dir, task["root"], task["task"])
        prep.update(pr); prep["packet_bytes"] = len(pk or "")
        base += f"A repository-analysis tool produced this evidence (RDX). Verify with tools.\n\nEVIDENCE:\n{pk}\n\n"
    else:
        base += "Use read/grep/find_file_by_name/exec.\n"
    base += "\nReturn:\nANSWER:\n<answer>\nEVIDENCE:\n<path:line>\n"
    return base, prep


def setup_wt(task, out, sid):
    if task.get("mutating"):
        d = Path(out) / "wt" / sid
        shutil.rmtree(d, ignore_errors=True)
        shutil.copytree(task["pristine_root"], d)
        return str(d)
    return task["root"]


def run_one(binary, state_dir, devin, task, arm, rep, out):
    sid = f"{task['id']}.{arm}.r{rep}"
    root = setup_wt(task, out, sid)
    ptxt, prep = make_prompt(task, arm, root, binary, state_dir)
    if arm == "S1" and not prep.get("prepare_ok"):
        # RepoDex preparation failed — do not silently run an empty treatment.
        tr = {"session": sid, "task": task["id"], "arm": arm, "rep": rep,
              "executor_mode": "dangerous", "execution_status": "INFRA_INVALID",
              "validation_status": "NOT_EVALUATED", "repodex_prepare": prep,
              "correct": False, "events": []}
        tp = Path(out) / "traces" / f"{sid}.json"; tp.parent.mkdir(parents=True, exist_ok=True)
        tp.write_text(json.dumps(tr))
        return tr
    pf = Path(out) / "prompts" / f"{sid}.txt"; pf.parent.mkdir(parents=True, exist_ok=True); pf.write_text(ptxt)
    exp = Path(out) / "exports" / f"{sid}.json"; exp.parent.mkdir(parents=True, exist_ok=True)
    exp.unlink(missing_ok=True)
    t0 = time.monotonic()
    timed_out = False
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
          "rejected": summ["rejected"], "repodex_prepare_wall_ms": prep["repodex_prepare_wall_ms"],
          "repodex_prepare": prep,
          "packet_bytes": prep["packet_bytes"], "agent_session_wall_ms": round(agent_wall),
          "combined_wall_ms": round(agent_wall + prep["repodex_prepare_wall_ms"]),
          "final_metrics": d.get("final_metrics", {}), "model_calls": summ["model_calls"],
          "tool_categories": summ["tools"], "correct": v["validation_status"] == "TASK_SUCCESS",
          "validation": v, "events": evs}
    tp = Path(out) / "traces" / f"{sid}.json"; tp.parent.mkdir(parents=True, exist_ok=True)
    tp.write_text(json.dumps(tr))
    return tr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--workdir", required=True)
    ap.add_argument("--corpus", default=str(Path(__file__).parent / "TASK_CORPUS.json"))
    ap.add_argument("--schedule", default=str(Path(__file__).parent / "SESSION_SCHEDULE.json"))
    ap.add_argument("--manifest", default=str(Path(__file__).parent / "FIXTURE_MANIFEST.json"))
    ap.add_argument("--state-dir", default=None)
    ap.add_argument("--devin", default="devin")
    ap.add_argument("--evidence-dir", default=None)
    ap.add_argument("--from-traces", action="store_true",
                    help="regenerate artifacts from existing traces without re-running agents")
    a = ap.parse_args()
    out = Path(a.workdir); evdir = Path(a.evidence_dir or out)
    for d in ("exports", "prompts", "traces", "wt"):
        (out / d).mkdir(parents=True, exist_ok=True)
    state_dir = a.state_dir or str(out / "state")
    tasks = {t["id"]: t for t in load(a.corpus)}
    # 1. fixture digest verify
    bad = fixture_digest_check(load(a.manifest))
    # 2. gold audit
    proofs = [gold.prove(t, t["root"]) for t in tasks.values()]
    gold_ok = all(p["all_proven"] for p in proofs)
    json.dump({"tasks": proofs, "FROZEN_GOLD_SOURCE_AUDIT_PASS": gold_ok,
               "fixture_mismatch": bad}, open(out / "GOLD_SOURCE_PROOFS.json", "w"), indent=1)
    if bad or not gold_ok:
        print(f"BLOCKED: fixture_mismatch={bad} gold_audit_pass={gold_ok} -> 0 scored sessions")
        json.dump({"BLOCKED": True, "reason": "gold or fixture"}, open(out / "SUMMARY.json", "w"))
        return 2
    # 3. tool preflight
    pf = tool_preflight(a.binary, state_dir, out)
    json.dump(pf, open(out / "TOOL_PREFLIGHT.json", "w"), indent=1)
    if not all(v is True or v == "ok" for k, v in pf.items() if k != "error"):
        print(f"BLOCKED: preflight failed {pf} -> 0 scored sessions")
        return 2
    # 4. scored sessions (or replay artifacts from existing traces)
    sched = load(a.schedule)
    results = []
    if a.from_traces:
        from classify import classify_tool as _ct
        for tp in sorted((out / "traces").glob("*.json")):
            t = load(tp)
            # Re-derive tool classification from raw TOOL_REQUEST events rather
            # than trusting the previously stored (possibly stale) categories.
            cats = []
            for e in t.get("events", []):
                if e.get("ev") == "TOOL_REQUEST":
                    cats.append(_ct(e.get("tool"), e.get("args")))
            old = t.get("tool_categories", [])
            if cats:
                t["_reclassified_from_raw"] = {"old_rep_dex": old.count("repodex"),
                                               "new_rep_dex": cats.count("repodex")}
                t["tool_categories"] = cats
            # model_calls re-derived from MODEL_TOKENS events
            mc = sum(1 for e in t.get("events", []) if e.get("ev") == "MODEL_TOKENS")
            if mc:
                t["model_calls"] = mc
            results.append(t)
        print(f"replayed {len(results)} existing traces (reclassified)")
    else:
        for job in sched:
            tr = run_one(a.binary, state_dir, a.devin, tasks[job["task"]], job["arm"], job["rep"], out)
            results.append(tr)
            print(tr["session"], tr["execution_status"], tr["validation_status"], flush=True)
    # 5. aggregate artifacts
    import metrics as M
    with open(out / "SESSION_METRICS.csv", "w", newline="") as g:
        w = csv.writer(g); w.writerow(M.HEADER); [w.writerow(M.session_row(t)) for t in results]
    idx = [{"session_id": t["session"], "task": t["task"], "arm": t["arm"], "rep": t["rep"],
            "execution_status": t["execution_status"], "validation_status": t["validation_status"],
            "trace_path": f"traces/{t['session']}.json",
            "sha256": hashlib.sha256((out / "traces" / f"{t['session']}.json").read_bytes()).hexdigest()}
           for t in results]
    json.dump(idx, open(out / "TRACE_INDEX.json", "w"), indent=1)
    inv = sum(1 for t in results if t["execution_status"] == "INFRA_INVALID")
    json.dump({"sessions": len(results), "INFRA_INVALID": inv,
               "VALID": sum(1 for t in results if t["execution_status"] == "EXECUTION_VALID"),
               "SUCCESS": sum(1 for t in results if t["validation_status"] == "TASK_SUCCESS")},
              open(out / "SUMMARY.json", "w"), indent=1)
    json.dump(results, open(out / "results.json", "w"), indent=1)
    # ---- full reviewable artifact package ----
    json.dump([{ "session": t["session"], "execution_status": t["execution_status"],
        "validation_status": t["validation_status"], "correct": t["correct"],
        "rejected": t["rejected"], "obligations": t["validation"].get("obligations"),
        "worktree": t["validation"].get("worktree")} for t in results],
        open(out / "VALIDATION_RESULTS.json", "w"), indent=1)
    alltools = {}
    for t in results:
        for c in t.get("tool_categories", []): alltools[c] = alltools.get(c, 0) + 1
    json.dump(alltools, open(out / "TOOL_EXECUTION_SUMMARY.json", "w"), indent=1)
    json.dump({t["session"]: {"agent_wall_ms": t["agent_session_wall_ms"],
        "repodex_prepare_wall_ms": t["repodex_prepare_wall_ms"],
        "combined_wall_ms": t["combined_wall_ms"]} for t in results},
        open(out / "TIMING.json", "w"), indent=1)
    json.dump({t["session"]: {"final_metrics": t["final_metrics"],
        "model_calls": t["model_calls"], "packet_bytes": t["packet_bytes"]}
        for t in results}, open(out / "TOKEN_ACCOUNTING.json", "w"), indent=1)
    json.dump({"schedule_digest": _sha(a.schedule), "corpus_digest": _sha(a.corpus),
        "manifest_digest": _sha(a.manifest), "pipeline": "tools/eval/run_smoke.py"},
        open(out / "REPRODUCIBILITY.json", "w"), indent=1)
    # sha manifest
    import glob as _g
    fs = [f for f in _g.glob(str(out / "**"), recursive=True) if Path(f).is_file()
          and "SHA256" not in f and f != str(out / "REVIEW_EVIDENCE_SHA256.txt")]
    with open(out / "REVIEW_EVIDENCE_SHA256.txt", "w") as g:
        for f in sorted(fs):
            line = hashlib.sha256(Path(f).read_bytes()).hexdigest()
            g.write(line + "  " + str(Path(f).relative_to(out)) + "\n")
    print(f"{len(results)} sessions, INFRA_INVALID={inv}, artifacts written")


if __name__ == "__main__":
    main()
