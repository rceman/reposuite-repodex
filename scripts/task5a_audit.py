#!/usr/bin/env python3
"""TASK 5A independent audit of the investigation graph.

Independently re-derives the expected graph projection from the upstream
snapshot/links/candidate artifacts — WITHOUT using the graph builder — and
compares it to the persisted nodes/edges. Classifies per §47.

usage: task5a_audit.py <snapshot> <links> <candidates> <graph>
"""
import hashlib, json, random, sys
from collections import Counter

def nid(key):  # matches node_id_for
    return "gn-" + hashlib.sha256(("repodex-graph-node-v1\n" + key).encode()).hexdigest()[:16]

def fkey(p):  return f"file:{p}"
def dkey(p,i):return f"decl:{p}#{i}"
def ikey(p,i):return f"imp:{p}#{i}"
def ckey(p,i):return f"call:{p}#{i}"
def ekey(e):  return f"ent:{e}"

SNAP, LINKS, CAND, G = sys.argv[1:5]
man = json.load(open(f"{SNAP}/manifest.json"))
links = [json.loads(l) for l in open(f"{LINKS}/links.jsonl")]
ents = {e["entity_id"]: e for e in map(json.loads, open(f"{LINKS}/entities.jsonl"))}
cands = [json.loads(l) for l in open(f"{CAND}/call_candidates.jsonl")]
nodes = {n["node_id"]: n for n in map(json.loads, open(f"{G}/nodes.jsonl"))}
node_by_key = {n["key"]: n for n in nodes.values()}
edges = [json.loads(l) for l in open(f"{G}/edges.jsonl")]
edge_set = {(e["kind"], e["source"], e["target"]) for e in edges}
edges_by_src = {}
for e in edges: edges_by_src.setdefault(e["source"], []).append(e)

analyses = {f["relative_path"]: json.load(open(f"{SNAP}/files/{f['object_key']}.json"))
            for f in man["files"]}

cls = Counter(); mism = []
def expect_node(key, kind, label=""):
    n = node_by_key.get(key)
    if n is None: return "MISSING_NODE"
    if n["kind"] != kind: return "EXTRA_NODE" if False else "WRONG_EDGE_TARGET"
    return "CORRECT_NODE"

random.seed(7)
# ---- 1. file/declaration/import/call nodes + contains edges ----
samples = []
for rel, a in analyses.items():
    for d in a.get("declarations", []): samples.append(("decl", rel, d["declaration_id"]))
    for i in a.get("imports", []): samples.append(("imp", rel, i["import_id"]))
    for c in a.get("calls", []): samples.append(("call", rel, c["call_id"]))
random.shuffle(samples)
decl_checked = imp_checked = call_checked = 0
for kind, rel, fid in samples:
    if kind == "decl" and decl_checked >= 500: continue
    if kind == "imp" and imp_checked >= 500: continue
    if kind == "call" and call_checked >= 500: continue
    if kind == "decl": decl_checked += 1; key = dkey(rel, fid); nk = "declaration"
    elif kind == "imp": imp_checked += 1; key = ikey(rel, fid); nk = "import"
    else: call_checked += 1; key = ckey(rel, fid); nk = "call"
    r = expect_node(key, nk)
    if r != "CORRECT_NODE": cls[r] += 1; mism.append((rel, key, r)); continue
    cls["CORRECT_NODE"] += 1
    # contains edge file -> node
    fe = ("contains", nid(fkey(rel)), nid(key))
    if fe in edge_set: cls["CORRECT_FACT_EDGE"] += 1
    else: cls["MISSING_EDGE"] += 1; mism.append((rel, "missing contains", key))

# ---- 2. structural link edges ----
link_checked = 0
for l in links:
    if link_checked >= 500: break
    src = l["source"]
    sk = {"declaration": dkey, "import": ikey, "call": ckey}.get(src["fact_kind"])
    if not sk: continue
    skey = sk(src["relative_path"], src["fact_id"])
    if skey not in node_by_key: continue
    oc = l["outcome"]
    targets = ([oc["target"]] if "target" in oc else oc.get("candidates", []))
    for t in targets:
        if t["kind"] == "file": tk = fkey(t["relative_path"])
        elif t["kind"] == "declaration": tk = dkey(t["relative_path"], t["declaration_id"])
        elif t["kind"] == "structure": tk = ekey(t["entity_id"])
        else: continue
        if tk not in node_by_key: cls["MISSING_NODE"] += 1; mism.append((l["link_id"], "tgt", tk)); continue
        if (l["kind"], nid(skey), nid(tk)) in edge_set:
            cls["CORRECT_FACT_EDGE"] += 1; link_checked += 1
        else:
            cls["MISSING_EDGE"] += 1; mism.append((l["link_id"], l["kind"], skey))

# ---- 3. every candidate record -> expected edge count ----
single = mult = none = oos = 0
for r in cands:
    skey = ckey(r["source"]["relative_path"], r["source"]["fact_id"])
    oc = r["outcome"]["outcome"]
    src_id = nid(skey)
    cand_edges = [e for e in edges_by_src.get(src_id, []) if e["kind"] == "call_candidate"]
    if oc == "single_candidate":
        single += 1
        if len(cand_edges) == 1 and cand_edges[0]["evidence_class"] == "candidate":
            # target must be the exact decl node
            t = r["outcome"]["candidates"][0] if "candidates" in r["outcome"] else r["outcome"].get("candidate")
            tkey = dkey(t["relative_path"], t["declaration_id"])
            if cand_edges[0]["target"] == nid(tkey): cls["CORRECT_SINGLE_CANDIDATE_EDGE"] += 1
            else: cls["WRONG_EDGE_TARGET"] += 1; mism.append((r["record_id"], "wrong tgt"))
        else: cls["WRONG_EDGE_CLASS" if cand_edges else "MISSING_EDGE"] += 1; mism.append((r["record_id"], "single"))
    elif oc == "multiple_candidates":
        mult += 1
        ncand = len(r["outcome"]["candidates"])
        sets = {e.get("candidate_set_id") for e in cand_edges}
        if len(cand_edges) == ncand and len(sets) == 1 and all(e["evidence_class"]=="candidate" for e in cand_edges):
            cls["CORRECT_MULTIPLE_CANDIDATE_SET"] += 1
        else:
            cls["WRONG_EDGE_TARGET"] += 1; mism.append((r["record_id"], "multiple"))
    else:
        # NoCandidate / OutOfScope -> no candidate edge.
        if cand_edges: cls["CANDIDATE_PROMOTED_TO_FACT" if oc=="out_of_scope" else "WRONG_EDGE_TARGET"] += 1
        else: cls["CORRECT_NO_TARGET_EDGE"] += 1
        if oc == "no_candidate": none += 1
        else: oos += 1

print("\n=== TASK 5A investigation-graph audit ===")
print(f"nodes={len(nodes)} edges={len(edges)}")
print(f"sampled decl/imp/call nodes: {decl_checked}/{imp_checked}/{call_checked}; link edges checked: {link_checked}")
print(f"candidate records: single={single} multiple={mult} none={none} oos={oos}")
for k, v in cls.most_common(): print(f"  {k:36} {v}")
# candidate promoted to fact check: no candidate edge has fact class
bad = [e for e in edges if e["kind"] == "call_candidate" and e["evidence_class"] != "candidate"]
print("call_candidate edges with non-candidate class:", len(bad))
if bad or any(k in cls for k in ("MISSING_NODE","EXTRA_NODE","WRONG_EDGE_TARGET","WRONG_EDGE_CLASS","CANDIDATE_PROMOTED_TO_FACT","MISSING_EDGE")):
    print("\nsample mismatches:")
    for m in mism[:30]: print("  !", m)
    print("FAIL"); sys.exit(1)
print("\nPASS")
