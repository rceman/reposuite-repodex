#!/usr/bin/env python3
"""Instrumented native-agent benchmark driver.

Spawns a fresh `devin -p --export` zero-context session per (question,treatment,rep).
Parses the ATIF export for real telemetry: prompt/completion tokens, tool_calls,
observations (real tool output), timestamps, model name.
"""
import json, os, re, subprocess, sys, time, hashlib
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

ROOT = Path("/tmp/repodex-jev-gtw-v2")
RAW = ROOT / "raw"; EXP = RAW / "exports"; PR = ROOT / "prompts"
PACKETS = ROOT / "packets"
GTW = "/tmp/gtw-bench"
Q = {q["id"]: q for q in json.load(open(ROOT.parent/"git/reposuite-repodex/docs/evals/jev-gtw-v2/questions.json" if False else "/home/therceman/git/reposuite-repodex/docs/evals/jev-gtw-v2/questions.json"))["questions"]}
QIDS = [f"L{l}-Q{n}" for l in (1,2,3,4) for n in (1,2,3,4,5)]
TREAT = ["E0","E1","E2","E3"]
PKT_MODE = {"E1":"A","E2":"C","E3":"D"}
REPS = 3
CONC = int(os.environ.get("BENCH_CONC","4"))
TIMEOUT = 600  # generous safety cap; not a latency gate

BASE = """You are investigating a pinned read-only repository checkout to answer a single question accurately.

REPOSITORY: {gtw}  (a Go codebase)

QUESTION:
{q}

INSTRUCTIONS:
- Investigate the repository and base every factual claim on source evidence you actually read from this checkout.
- Use the read/grep/glob (find_file_by_name) tools to explore the source.
- Do NOT use exec, web_search, webfetch, run_subagent, ask_user_question, or any external "repodex"/"reposuite" tool. Do not ask the user questions; answer directly.
- Cite exact source paths and relevant symbols for each claim. Do not guess unsupported facts.
{pkt}
Return exactly:
ANSWER:
<concise complete answer>
EVIDENCE:
- <path> :: <symbol if known> :: <reason>
"""

PKT_BLOCK = """
A precomputed repository retrieval packet is provided below as a navigation aid. You may inspect those or other repository files as needed. Verify claims against source where appropriate.

<RETRIEVAL_PACKET>
{pkt}
</RETRIEVAL_PACKET>
"""

def run_id(qid, t, rep):
    return f"{qid}__{t}__r{rep}"

def build_prompt(qid, t):
    q = Q[qid]["query"]
    pkt = ""
    if t in PKT_MODE:
        mode = PKT_MODE[t]
        body = (PACKETS / f"{mode}-{qid}.rdx").read_text()
        pkt = PKT_BLOCK.format(pkt=body)
    return BASE.format(gtw=GTW, q=q, pkt=pkt)

def pkt_hash(qid, t):
    if t not in PKT_MODE: return None
    return hashlib.sha256((PACKETS/f"{PKT_MODE[t]}-{qid}.rdx").read_bytes()).hexdigest()[:16]

def one(qid, t, rep):
    rid = run_id(qid, t, rep)
    exp = EXP / f"{rid}.json"
    pf = PR / f"{rid}.txt"
    pf.write_text(build_prompt(qid, t))
    if exp.exists(): exp.unlink()
    t0 = time.time()
    try:
        p = subprocess.run(
            ["devin","-p","--prompt-file",str(pf),"--export",str(exp),
             "--respect-workspace-trust","false"],
            cwd=GTW, capture_output=True, text=True, timeout=TIMEOUT)
        wall = (time.time()-t0)*1000
        timed_out = False
    except subprocess.TimeoutExpired:
        wall = TIMEOUT*1000; timed_out=True
    rec = {"agent_run_id":rid,"question_id":qid,"treatment":t,"rep":rep,
           "subagent_created":True,"context_inheritance":False,
           "packet_sha":pkt_hash(qid,t),"wall_ms":round(wall,1),
           "timed_out":timed_out,"export":str(exp),"export_exists":exp.exists()}
    return rec

def schedule():
    # counterbalanced: rotate treatment start across questions within each rep
    out=[]
    for rep in range(REPS):
        for qi,qid in enumerate(QIDS):
            for k in range(4):
                t=TREAT[(qi+k)%4]
                out.append((qid,t,rep))
    return out

def main():
    EXP.mkdir(parents=True,exist_ok=True);PR.mkdir(parents=True,exist_ok=True)
    jobs = schedule()
    done = RAW/"agent_run_index.jsonl"
    existing = {json.loads(l)["agent_run_id"] for l in open(done)} if done.exists() else set()
    jobs = [j for j in jobs if run_id(*j) not in existing]
    print(f"remaining {len(jobs)} runs, conc={CONC}", flush=True)
    lf = open(done,"a")
    with ThreadPoolExecutor(max_workers=CONC) as ex:
        futs = {ex.submit(one,q,t,r):(q,t,r) for q,t,r in jobs}
        n=0
        for fu in as_completed(futs):
            rec = fu.result(); lf.write(json.dumps(rec)+"\n"); lf.flush()
            n+=1
            print(f"[{n}/{len(jobs)}] {rec['agent_run_id']} wall={rec['wall_ms']}ms export={rec['export_exists']} to={rec['timed_out']}", flush=True)
    lf.close(); print("DRIVER_DONE", flush=True)

if __name__=="__main__":
    main()
