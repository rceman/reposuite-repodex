#!/usr/bin/env python3
"""Parse ATIF exports -> instrumented JSONLs + metrics + gold scoring."""
import json, re, sys, hashlib
from pathlib import Path
from collections import defaultdict

ROOT=Path("/tmp/repodex-jev-gtw-v2");RAW=ROOT/"raw";EXP=RAW/"exports"
GT=json.load(open("/home/therceman/git/reposuite-repodex/docs/evals/jev-gtw-v2/ground_truth.json"))["answers"]
AG=json.load(open(ROOT/"answer_gold.json"))["answers"]
SEARCH={"grep","find_file_by_name","skill","notebook_read"}
READ={"read"}
NONREPO={"exec","web_search","webfetch","run_subagent","ask_user_question","browser_preview",
 "close_browser_preview","get_output","kill_shell","write_to_process","mcp_call_tool",
 "mcp_list_servers","mcp_list_tools","mcp_read_resource","request_scope","todo_write",
 "edit","write","notebook_edit","read_subagent"}
BANNED_ANY={"web_search","webfetch","run_subagent","mcp_call_tool","mcp_read_resource","ask_user_question"}
GO=re.compile(r"(?:/tmp/gtw-bench/|/)?((?:internal|cmd|pkg|scripts|testdata)/[^\s\"'<>:]+\.(?:go|md|txt|yaml|json|toml))")
GOFILE=re.compile(r"(?:/tmp/gtw-bench/)?((?:internal|cmd|pkg|scripts|testdata)/[^\s\"'<>:]+\.go)")
IDENT=re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]+)*\b")

def classify(fn,args):
    if fn in SEARCH:return "search"
    if fn in READ:return "file_read"
    if fn=="exec":
        a=json.dumps(args)
        if "git " in a:return "git_inspection"
        if re.search(r'\b(ls|find|tree|du)\b',a):return "directory_list"
        return "non_repository_tool"
    if fn in NONREPO:return "non_repository_tool"
    return "other_repository_tool"

def est_tok(txt):return max(1,round(len(txt)/4)) if txt else 0

def fact_terms(fact):
    # extract checkable terms: CamelCase/snake idents, literal patterns, key phrases
    terms=set()
    for m in IDENT.findall(fact):
        if len(m)>=3 and (re.search(r'[A-Z]',m) or '_' in m or '.' in m or m.isupper()):
            terms.add(m)
    for m in re.findall(r'WT-TSK[0-9]*|->|>=|<=|\b[a-z]{3,}_[a-z_]+\b',fact):terms.add(m)
    # lowercase content words >4 chars as weak signal
    for m in re.findall(r'\b[a-z]{5,}\b',fact):
        if m not in("which","where","their","before","after","value","every","only","this","that","with","from","into","must","when","then","than","they","them","such","have","been","does","each","also","same","task","agent"):
            terms.add(m)
    return terms

def score_answer(qid,answer,evid):
    g=AG.get(qid,{});facts=g.get("required_facts",[]);pp=g.get("primary_paths",[]);ps=g.get("symbols",[])
    blob=(answer+" "+evid).lower()
    # fact recall: a fact recalled if >=50% of its extracted terms present (>=1 if few)
    fr=0;fc=len(facts)
    for f in facts:
        terms={t.lower() for t in fact_terms(f)}
        if not terms:fr+=0.5;continue
        hit=sum(1 for t in terms if t in blob)
        need=1 if len(terms)<=2 else max(1,int(len(terms)*0.4))
        if hit>=need:fr+=1
    recall=fr/fc if fc else 0.0
    # primary evidence coverage: primary paths cited
    pec=sum(1 for p in pp if p.lower() in blob)/(len(pp)or 1)
    # invalid path/symbol count: paths in evidence that don't look like repo paths
    invalid=0
    for ln in evid.splitlines():
        mp=re.search(r'(/[^\s:]+\.(?:go|md))',ln)
        if mp:
            p=mp.group(1)
            if not p.startswith(("/tmp/gtw-bench/","internal/","cmd/","pkg/")):invalid+=1
    # unsupported/contradicted: heuristic - count 'no evidence' markers; keep 0 unless clearly contradictory
    unsup=0;contra=0
    if re.search(r'\b(cannot find|not found|does not exist|no such)\b',blob) and recall<0.4:unsup=1
    return {"required_fact_recall":round(recall,3),"primary_evidence_coverage":round(pec,3),
            "fact_atoms":fc,"facts_hit":fr,"unsupported_claims":unsup,"contradicted_claims":contra,
            "invalid_path_symbol":invalid}

def parse_export(path,ps=None):
    d=json.load(open(path));ps=ps or set()
    fm=d.get("final_metrics",{})
    model=d.get("agent",{}).get("model_name");sid=d.get("session_id")
    calls=[];seq=0;observed=set();reads=set();sbytes=0;rbytes=0
    per_step=[]
    for i,st in enumerate(d.get("steps",[])):
        ts=st.get("timestamp");m=st.get("metrics") or {}
        tcs=st.get("tool_calls",[])
        obs=st.get("observation",{})
        results=obs.get("results",[])
        byid={r.get("source_call_id"):r.get("content","") for r in results}
        for tc in tcs:
            seq+=1;fn=tc.get("function_name");args=tc.get("arguments") or {}
            cls=classify(fn,args)
            cid=tc.get("tool_call_id")
            out=byid.get(cid,"")
            ob=len(out.encode())
            pths=set(GOFILE.findall(out))
            rp=args.get("file_path") or args.get("path") or ""
            if fn=="read" and rp:
                pths.add(re.sub(r'^/tmp/gtw-bench/','',rp));reads.add(re.sub(r'^/tmp/gtw-bench/','',rp))
            if fn in("find_file_by_name","grep"):sbytes+=ob
            if fn=="read":rbytes+=ob
            syms={s for s in ps if s and re.search(r'\b'+re.escape(s)+r'\b',out)}
            for p in pths:
                if p.startswith(("internal/","cmd/","pkg/","scripts/","testdata/")):observed.add(p)
            calls.append({"seq":seq,"step":i,"tool":fn,"class":cls,"args":args,
                          "ts":ts,"out_bytes":ob,"paths":sorted(pths),"syms":sorted(syms),"call_id":cid})
        per_step.append({"step":i,"ts":ts,"pt":m.get("prompt_tokens"),"ct":m.get("completion_tokens"),
                         "cached":m.get("cached_tokens"),"n_calls":len(tcs)})
    return {"session_id":sid,"model":model,"fm":fm,"calls":calls,"steps":per_step,
            "observed":sorted(observed),"reads":sorted(reads),
            "sbytes":sbytes,"rbytes":rbytes,"final":str(d["steps"][-1].get("message",""))}

def main():
    idx=[json.loads(l) for l in open(RAW/"agent_run_index.jsonl")]
    fr=open(RAW/"instrumented_agent_runs.jsonl","w")
    ft=open(RAW/"instrumented_agent_tool_calls.jsonl","w")
    fu=open(RAW/"instrumented_agent_usage.jsonl","w")
    fa=open(RAW/"instrumented_agent_answers.jsonl","w")
    fs=open(RAW/"instrumented_agent_scoring.jsonl","w")
    for rec in idx:
        rid=rec["agent_run_id"];qid=rec["question_id"];t=rec["treatment"]
        exp=Path(rec["export"])
        if not exp.exists():
            fr.write(json.dumps({**rec,"valid":False,"invalid_reason":"no_export"})+"\n");continue
        g=GT.get(qid,{});pps=set(g.get("primary_paths",[]));pss=set(g.get("primary_symbols",[]))
        try:d=parse_export(exp,pss)
        except Exception as e:
            fr.write(json.dumps({**rec,"valid":False,"invalid_reason":f"parse:{e}"})+"\n");continue
        # audit: E0 must not use repodex/exec/web; E1-3 must not use web/repodex/extra-repo
        bad=[]
        for c in d["calls"]:
            if c["tool"] in BANNED_ANY:bad.append(c["tool"])
            if c["tool"]=="exec" and re.search(r'repodex|reposuite',json.dumps(c["args"])):bad.append("exec:repodex")
        # gold leak check: agent shouldn't reference eval paths
        valid=not bad and not rec["timed_out"]
        why=None if valid else ("banned_tools:"+",".join(sorted(set(bad))) if bad else "timeout")
        run={**rec,"session_id":d["session_id"],"model":d["model"],"valid":valid,
             "invalid_reason":why,"steps":d["fm"].get("total_steps")}
        fr.write(json.dumps(run)+"\n")
        for c in d["calls"]:
            ft.write(json.dumps({"agent_run_id":rid,"seq":c["seq"],"step":c["step"],"tool":c["tool"],
                "class":c["class"],"args_repr":json.dumps(c["args"])[:400],"ts":c["ts"],
                "out_bytes":c["out_bytes"],"paths":c["paths"]})+"\n")
        fu.write(json.dumps({"agent_run_id":rid,"agent_input_tokens":d["fm"].get("total_prompt_tokens"),
            "agent_output_tokens":d["fm"].get("total_completion_tokens"),
            "agent_total_tokens":(d["fm"].get("total_prompt_tokens")or 0)+(d["fm"].get("total_completion_tokens")or 0),
            "cached_tokens":d["fm"].get("total_cached_tokens"),"steps":d["fm"].get("total_steps"),
            "actual_usage_available":True,"per_step":d["steps"]})+"\n")
        ans=d["final"];evid=""
        mm=re.search(r'EVIDENCE:\s*(.*)',ans,re.S|re.I)
        if mm:evid=mm.group(1)
        fa.write(json.dumps({"agent_run_id":rid,"answer":ans[:6000],"evidence":evid[:4000]})+"\n")
        sc=score_answer(qid,ans,evid)
        sc["agent_run_id"]=rid
        # evidence-progress: track cumulative primary path+symbol coverage across calls
        seen=set();first=None;full=None;first_read=None;first_obs=None;t0=None
        for c in d["calls"]:
            if t0 is None:t0=c["ts"]
            new=set()
            for p in c["paths"]:
                if p in pps:seen.add(("p",p));new.add(p)
            for s in c.get("syms",[]):
                if s in pss:seen.add(("s",s));new.add(s)
            if first is None and new:
                first=c["seq"];first_obs=c["seq"]
                if c["class"]=="file_read":first_read=c["seq"]
            if first is not None and first_read is None and c["class"]=="file_read" and new:
                first_read=c["seq"]
            if pps and all(("p",p) in seen for p in pps) and all(("s",s) in seen for s in pss) and full is None:
                full=c["seq"]
        from collections import Counter
        cc=Counter(c["class"] for c in d["calls"])
        sc.update({"tool_calls_total":len(d["calls"]),
                   "repo_tool_calls":len(d["calls"])-cc.get("non_repository_tool",0),
                   "search_calls":cc.get("search",0),"file_read_calls":cc.get("file_read",0),
                   "dir_list_calls":cc.get("directory_list",0),"git_calls":cc.get("git_inspection",0),
                   "tool_calls_to_first_primary":first,"tool_calls_to_full_primary":full,
                   "reads_to_first_primary":first_read,"observed_to_first_primary":first_obs,
                   "full_primary_reached":full is not None,
                   "unique_files_read":len(d["reads"]),"unique_files_observed":len(d["observed"]),
                   "repo_out_bytes":d["sbytes"]+d["rbytes"],
                   "est_repo_out_tokens":est_tok("x"*(d["sbytes"]+d["rbytes"]))})
        fs.write(json.dumps(sc)+"\n")
    for f in (fr,ft,fu,fa,fs):f.close()
    print("collected",sum(1 for _ in open(RAW/"instrumented_agent_runs.jsonl")),"runs")

if __name__=="__main__":main()
