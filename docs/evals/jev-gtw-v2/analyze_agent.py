#!/usr/bin/env python3
"""Aggregate instrumented agent runs -> AGENT_SUMMARY.json + comparisons."""
import json,statistics,math
from pathlib import Path
from collections import defaultdict
RAW=Path("/tmp/repodex-jev-gtw-v2/raw")
Q=json.load(open("/home/therceman/git/reposuite-repodex/docs/evals/jev-gtw-v2/questions.json"))["questions"]
QL={q["id"]:q["level"] for q in Q}
RETR={json.loads(l)["run_id"]:json.loads(l) for l in open(RAW/"retrieval_runs.jsonl")}
# frozen retrieval ms: rep0 of matching mode per question
MODE={"E1":"A","E2":"C","E3":"D","E0":None}
RETMS={}
for rid,r in RETR.items():
    if r.get("rep")==0: RETMS[(r["qid"],r["mode"])]=r.get("e2e_ms",0)

runs={json.loads(l)["agent_run_id"]:json.loads(l) for l in open(RAW/"instrumented_agent_runs.jsonl")}
usage={json.loads(l)["agent_run_id"]:json.loads(l) for l in open(RAW/"instrumented_agent_usage.jsonl")}
scor={json.loads(l)["agent_run_id"]:json.loads(l) for l in open(RAW/"instrumented_agent_scoring.jsonl")}

def M(rid):
    u=usage.get(rid,{});s=scor.get(rid,{});r=runs[rid]
    mode=MODE[r["treatment"]]
    ret_ms=RETMS.get((r["question_id"],mode),0) if mode else 0
    return {
     "recall":s.get("required_fact_recall"),"pec":s.get("primary_evidence_coverage"),
     "unsup":s.get("unsupported_claims",0),"contra":s.get("contradicted_claims",0),"invalid":s.get("invalid_path_symbol",0),
     "tct":s.get("tool_calls_total"),"rtc":s.get("repo_tool_calls"),"search":s.get("search_calls"),
     "read":s.get("file_read_calls"),"ufr":s.get("unique_files_read"),"ufo":s.get("unique_files_observed"),
     "in_tok":u.get("agent_input_tokens"),"out_tok":u.get("agent_output_tokens"),"tot_tok":u.get("agent_total_tokens"),
     "cached":u.get("cached_tokens"),"repo_bytes":s.get("repo_out_bytes"),"repo_tok":s.get("est_repo_out_tokens"),
     "wall":r.get("wall_ms"),"e2e":(r.get("wall_ms")or 0)+ret_ms,"full":s.get("full_primary_reached")}

valid=[r for r in runs.values() if r.get("valid")]
byt=defaultdict(list)
for r in valid:byt[r["treatment"]].append(r["agent_run_id"])
def agg(rids):
    ms=[M(r) for r in rids]
    def a(k):v=[m[k] for m in ms if m[k] is not None];return round(statistics.mean(v),2) if v else None
    return {k:a(k) for k in["recall","pec","unsup","contra","invalid","tct","rtc","search","read","ufr","ufo","in_tok","out_tok","tot_tok","cached","repo_bytes","repo_tok","wall","e2e"]} | {"n":len(rids),"full_reach":round(statistics.mean([1.0 if m["full"] else 0 for m in ms]),2)}

S={"n_valid":len(valid),"n_total":len(runs),
   "invalid":[{"run":r["agent_run_id"],"why":r.get("invalid_reason")} for r in runs.values() if not r.get("valid")],
   "per_treatment":{t:agg(byt[t]) for t in["E0","E1","E2","E3"]},
   "per_level":{},"per_question":{},"comparisons":{},"variance":{}}

# per-level
for lvl in[1,2,3,4]:
    for t in["E0","E1","E2","E3"]:
        rs=[r["agent_run_id"] for r in valid if r["treatment"]==t and QL[r["question_id"]]==lvl]
        if rs:S["per_level"][f"{t}-L{lvl}"]=agg(rs)
# per-question
for qid in sorted(QL):
    S["per_question"][qid]={t:agg([r["agent_run_id"] for r in valid if r["treatment"]==t and r["question_id"]==qid]) for t in["E0","E1","E2","E3"]}
# comparisons
def cmp(a,b):
    A=S["per_treatment"][a];B=S["per_treatment"][b];out={}
    for k in["recall","pec","tct","search","read","ufr","ufo","in_tok","out_tok","tot_tok","repo_bytes","repo_tok","wall","e2e"]:
        if A[k]is not None and B[k]is not None:
            out[k]={"a":A[k],"b":B[k],"delta":round(A[k]-B[k],2),"pct":round((A[k]-B[k])/B[k]*100,1) if B[k] else None}
    return out
for pair in["E1_E0","E2_E1","E3_E2","E3_E1","E2_E0","E3_E0"]:
    a,b=pair.split("_");S["comparisons"][pair]=cmp(a,b)
# variance across the 3 reps (mean of per-question population variance)
for t in["E0","E1","E2","E3"]:
    for k in["recall","tct","read","ufo","in_tok","out_tok","tot_tok","wall"]:
        qv=[]
        for qid in QL:
            ms=[M(r["agent_run_id"])[k] for r in valid if r["treatment"]==t and r["question_id"]==qid and M(r["agent_run_id"])[k] is not None]
            if len(ms)>1:qv.append(statistics.pvariance(ms))
        S["variance"].setdefault(t,{})[k]=round(statistics.mean(qv),3) if qv else None

json.dump(S,open("/tmp/repodex-jev-gtw-v2/AGENT_SUMMARY.json","w"),indent=1)
print("valid",S["n_valid"],"invalid",len(S["invalid"]))
for t in["E0","E1","E2","E3"]:
    x=S["per_treatment"][t]
    print(t,"n",x["n"],"recall",x["recall"],"pec",x["pec"],"tct",x["tct"],"in_tok",x["in_tok"],"out_tok",x["out_tok"],"e2e",x["e2e"])
print("E1-E0 in_tok delta:",S["comparisons"]["E1_E0"]["in_tok"])
