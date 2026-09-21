#!/usr/bin/env python3
"""TASK 4C independent audit of Go package-local plain-name call candidates.

Re-derives each plain_name call's expected outcome from the persisted snapshot
facts + TASK 4A go_package topology + TASK 4B bindings + package declarations —
WITHOUT calling production candidate derivation. Compares against the emitted
candidate records and classifies per §40.
"""
import json, sys
from collections import Counter, defaultdict

SNAP = sys.argv[1] if len(sys.argv) > 1 else "/tmp/hugo-snap-t4b"
LINKS = sys.argv[2] if len(sys.argv) > 2 else "/tmp/hugo-links-t4b"
CAND = sys.argv[3] if len(sys.argv) > 3 else "/tmp/hugo-cand-t4c"

manifest = json.load(open(f"{SNAP}/manifest.json"))
# file -> go_package entity (canonical package identity); import_path -> pkg name
file_pkg = {}
imp_to_name = {}
pkg_decls = defaultdict(list)  # pkg_key -> [(kind, name, path)]
for l in open(f"{LINKS}/entities.jsonl"):
    e = json.loads(l)
    if e["structural_kind"] != "go_package":
        continue
    for f in e["files"]:
        file_pkg[f] = e["key"]
    ip = next((a.split("=",1)[1] for a in e["assumptions"] if a.startswith("import_path=")), None)
    name = e["key"].rsplit(":",1)[1]
    if ip:
        imp_to_name[ip] = name

# per-file analysis
def load(f):
    return json.load(open(f"{SNAP}/files/{f['object_key']}.json"))

# package key -> member files (for decl collection)
pkg_files = defaultdict(list)
for f, k in file_pkg.items():
    pkg_files[k].append(f)

# Build package -> decls by loading member files once.
analyses = {}
for f in manifest["files"]:
    if f["language"] == "go":
        analyses[f["relative_path"]] = (f, load(f))

pkg_decl_index = defaultdict(list)  # pkg_key -> [(name, kind)]
for pk, files in pkg_files.items():
    for rel in files:
        a = analyses.get(rel)
        if not a:
            continue
        for d in a[1].get("declarations", []):
            pkg_decl_index[pk].append((d["name"], d["kind"]))

# Import local names per file
def import_local_names(a):
    names = set(); dot = False
    for imp in a.get("imports", []):
        for it in imp["items"]:
            cat = it.get("category", "normal")
            if cat == "dot":
                dot = True
            elif cat == "blank":
                continue
            elif it.get("alias"):
                names.add(it["alias"])
            elif it["target"] in imp_to_name:
                names.add(imp_to_name[it["target"]])
    return names, dot

def expected(a, file_key, call):
    """Independently derive the expected outcome for one plain_name call."""
    name = call["callee_written"]
    pos = call["callee_range"]["byte_start"]
    # local binding covering?
    for b in a.get("bindings", []):
        if b["name"] == name and any(v["byte_start"] <= pos < v["byte_end"] for v in b["visibility_ranges"]):
            return ("no_candidate", "shadowed_by_local_binding", [])
    names, dot = import_local_names(a)
    if dot:
        return ("no_candidate", "dot_import_namespace_uncertain", [])
    if name in names:
        return ("no_candidate", "blocked_by_import_binding", [])
    pk = file_pkg.get(file_key)
    if pk is None:
        return ("no_candidate", "no_package_context", [])
    funcs = [(n, k) for (n, k) in pkg_decl_index.get(pk, []) if n == name and k == "function"]
    nonfunc = [(n, k) for (n, k) in pkg_decl_index.get(pk, []) if n == name and k != "function"]
    if nonfunc:
        return ("no_candidate", "package_namespace_ambiguous", [])
    if len(funcs) == 0:
        return ("no_candidate", "no_package_function", [])
    if len(funcs) == 1:
        return ("single_candidate", None, funcs)
    return ("multiple_candidates", None, funcs)

# production records keyed by (path, call_id)
prod = {}
for l in open(f"{CAND}/call_candidates.jsonl"):
    r = json.loads(l)
    if r["rule_id"] != "go.call.package_local_function_candidate":
        continue
    prod[(r["source"]["relative_path"], r["source"]["fact_id"])] = r

cls = Counter()
reasons = Counter()
ext_test = Counter()
main_pkg = Counter()
mismatches = []
total = 0
for f in manifest["files"]:
    if f["language"] != "go":
        continue
    rel = f["relative_path"]
    a = analyses[rel][1]
    pk = file_pkg.get(rel)
    is_ext_test = pk and pk.rsplit(":",1)[1].endswith("_test")
    is_main = pk and pk.rsplit(":",1)[1] == "main"
    for call in a["calls"]:
        if call["form"] != "plain_name":
            continue
        total += 1
        exp_out, exp_reason, _ = expected(a, rel, call)
        rec = prod.get((rel, call["call_id"]))
        got = rec["outcome"]["outcome"] if rec else "<missing>"
        got_reason = rec["outcome"].get("reason") if rec and rec["outcome"]["outcome"]=="no_candidate" else None
        if is_ext_test: ext_test[got if got!="no_candidate" else f"no:{got_reason}"] += 1
        if is_main: main_pkg[got] += 1
        reasons[got if got!="no_candidate" else f"no:{got_reason}"] += 1
        # classification
        if rec is None:
            cls["MISSING_RECORD"] += 1; mismatches.append((rel,call["callee_written"],"missing"))
        elif got == exp_out and (exp_out!="no_candidate" or got_reason==exp_reason):
            if exp_out == "single_candidate":
                cls["CORRECT_SINGLE_PACKAGE_FUNCTION"] += 1
            elif exp_out == "multiple_candidates":
                cls["CORRECT_MULTIPLE_PACKAGE_FUNCTION"] += 1
            else:
                if exp_reason == "package_namespace_ambiguous":
                    cls["PACKAGE_NAMESPACE_AMBIGUITY_CORRECT"] += 1
                else:
                    cls["CORRECT_NO_CANDIDATE"] += 1
        else:
            # misclassification
            if got in ("single_candidate","multiple_candidates") and exp_out=="no_candidate":
                if exp_reason=="shadowed_by_local_binding": cls["LOCAL_SHADOWING_ERROR"]+=1
                elif exp_reason in ("blocked_by_import_binding","dot_import_namespace_uncertain"): cls["IMPORT_SHADOWING_ERROR"]+=1
                elif exp_reason=="package_namespace_ambiguous": cls["PACKAGE_NAMESPACE_AMBIGUITY_ERROR"]+=1
                else: cls["FALSE_PACKAGE_FUNCTION_CANDIDATE"]+=1
                mismatches.append((rel,call["callee_written"],f"got={got} exp=no:{exp_reason}"))
            elif got=="no_candidate" and exp_out in ("single_candidate","multiple_candidates"):
                cls["MISSING_PACKAGE_FUNCTION_CANDIDATE"]+=1
                mismatches.append((rel,call["callee_written"],f"got=no:{got_reason} exp={exp_out}"))
            elif got in ("single_candidate","multiple_candidates") and exp_out in ("single_candidate","multiple_candidates") and got!=exp_out:
                cls["WRONG_PACKAGE_FUNCTION_TARGET"]+=1
                mismatches.append((rel,call["callee_written"],f"got={got} exp={exp_out}"))
            else:
                cls["REASON_MISMATCH"]+=1
                mismatches.append((rel,call["callee_written"],f"got={got}:{got_reason} exp={exp_out}:{exp_reason}"))

print("\n=== TASK 4C Go plain-name candidate audit ===")
print("plain_name calls audited:", total)
for k,v in cls.most_common(): print(f"  {k:44} {v}")
print("\nproduction outcome/reason breakdown:")
for k,v in reasons.most_common(): print(f"  {k:44} {v}")
print("\nexternal-test package calls:", dict(ext_test))
print("package main calls:", dict(main_pkg))
if mismatches:
    print("\nsample mismatches:")
    for m in mismatches[:40]: print("  !", m)
    print("FAIL"); sys.exit(1)
print("\nPASS")
