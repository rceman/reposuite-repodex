#!/usr/bin/env python3
"""TASK 4D independent audit of Go imported package-qualified call candidates.

Re-derives each `member_selector` call's expected outcome from persisted facts +
TASK 4A topology + import links + TASK 4B bindings — WITHOUT production
candidate code. Classifies per §43.
"""
import json, sys
from collections import Counter, defaultdict

SNAP, LINKS, CAND = (sys.argv[1:] + ["/tmp/hugo-snap-t4b","/tmp/hugo-links-t4b","/tmp/hugo-cand-t4d"])[:3]

manifest = json.load(open(f"{SNAP}/manifest.json"))
# entity_id -> go_package entity; file -> package key
ent_by_id = {}
file_pkg = {}
pkg_files = defaultdict(list)
for l in open(f"{LINKS}/entities.jsonl"):
    e = json.loads(l)
    if e["structural_kind"] == "go_package":
        ent_by_id[e["entity_id"]] = e
        for f in e["files"]:
            file_pkg[f] = e["key"]
for f, k in file_pkg.items():
    pkg_files[k].append(f)

# import item -> target package entities (from import_path links)
import_link = {}
for l in open(f"{LINKS}/links.jsonl"):
    r = json.loads(l)
    if r["kind"] != "import_path":
        continue
    s = r["source"]
    targets = []
    oc = r["outcome"]
    for t in ([oc["target"]] if "target" in oc else oc.get("candidates", [])):
        if t.get("structural_kind") == "go_package" and t.get("entity_id") in ent_by_id:
            targets.append(ent_by_id[t["entity_id"]])
    import_link[(s["relative_path"], s["fact_id"], s["item_index"])] = targets

analyses = {}
for f in manifest["files"]:
    if f["language"] == "go":
        analyses[f["relative_path"]] = json.load(open(f"{SNAP}/files/{f['object_key']}.json"))

# package key -> decls
pkg_decls = defaultdict(list)
for pk, files in pkg_files.items():
    for rel in files:
        for d in analyses.get(rel, {}).get("declarations", []):
            pkg_decls[pk].append((d["name"], d["kind"]))

def is_ident(s):
    return s and (s[0].isalpha() or s[0] == "_") and all(c.isalnum() or c == "_" for c in s)

def is_exported(s):
    return bool(s) and s[0].isupper()

def file_imports(a):
    """local_name -> (packages, external)."""
    roots = {}
    dot = False
    for imp in a.get("imports", []):
        for i, it in enumerate(imp["items"]):
            cat = it.get("category", "normal")
            if cat == "dot":
                dot = True; continue
            if cat == "blank":
                continue
            pkgs = import_link.get((imp["relative_path"], imp["import_id"], i), [])
            if it.get("alias"):
                name = it["alias"]
            elif pkgs:
                name = pkgs[0]["key"].rsplit(":", 1)[1]
            else:
                name = it["target"].rsplit("/", 1)[-1]
            roots[name] = (pkgs, not pkgs)
    return roots, dot

def covers(a, name, pos):
    return any(b["name"] == name and any(v["byte_start"] <= pos < v["byte_end"] for v in b["visibility_ranges"]) for b in a.get("bindings", []))

def expected(a, rel, call):
    w = call["callee_written"]
    if "." not in w:
        return "out_of_scope", "not_a_direct_package_selector"
    root, term = w.rsplit(".", 1)
    if not (is_ident(root) and is_ident(term)):
        return "out_of_scope", "not_a_direct_package_selector"
    roots, _ = file_imports(a)
    if root not in roots:
        return "out_of_scope", "selector_root_not_an_import"
    pkgs, external = roots[root]
    if covers(a, root, call["callee_range"]["byte_start"]):
        return "no_candidate", "import_root_shadowed_by_local_binding"
    if external:
        return "out_of_scope", "external_import"
    if not is_exported(term):
        return "no_candidate", "imported_function_not_exported"
    funcs = 0
    nonfunc = 0
    for p in pkgs:
        for (n, k) in pkg_decls.get(p["key"], []):
            if n == term:
                if k == "function": funcs += 1
                else: nonfunc += 1
    if nonfunc:
        return "no_candidate", "imported_package_namespace_ambiguous"
    if funcs == 0:
        return "no_candidate", "no_imported_function"
    if funcs == 1:
        return "single_candidate", None
    return "multiple_candidates", None

prod = {}
for l in open(f"{CAND}/call_candidates.jsonl"):
    r = json.loads(l)
    if r["rule_id"] == "go.call.imported_package_function_candidate":
        prod[(r["source"]["relative_path"], r["source"]["fact_id"])] = r

cls = Counter(); reasons = Counter(); ext_test = Counter(); alias_c = Counter(); mism = []
total = 0; repo_local = 0
for f in manifest["files"]:
    if f["language"] != "go": continue
    rel = f["relative_path"]; a = analyses[rel]
    pk = file_pkg.get(rel, "")
    is_ext = pk.rsplit(":", 1)[1].endswith("_test")
    for call in a["calls"]:
        if call["form"] != "member_selector": continue
        total += 1
        exp_out, exp_reason = expected(a, rel, call)
        rec = prod.get((rel, call["call_id"]))
        got = rec["outcome"]["outcome"] if rec else "<missing>"
        got_reason = rec["outcome"].get("reason") if rec and rec["outcome"]["outcome"]=="no_candidate" else \
                     (rec["outcome"].get("reason") if rec and rec["outcome"]["outcome"]=="out_of_scope" else None)
        w = call["callee_written"]; root = w.split(".")[0] if "." in w else w
        roots,_ = file_imports(a)
        is_repo = root in roots and not roots[root][1]
        if is_repo: repo_local += 1
        if is_ext: ext_test[got]+=1
        # alias audit: root is an aliased import
        reasons[f"{got}:{got_reason or ''}"] += 1
        if rec is None:
            cls["MISSING_RECORD"] += 1; mism.append((rel,w,"missing")); continue
        if got == exp_out and (exp_out!="no_candidate" or got_reason==exp_reason):
            if exp_out=="single_candidate": cls["CORRECT_SINGLE_IMPORTED_PACKAGE_FUNCTION"]+=1
            elif exp_out=="multiple_candidates": cls["CORRECT_MULTIPLE_IMPORTED_PACKAGE_FUNCTION"]+=1
            elif exp_reason=="external_import": cls["CORRECT_EXTERNAL_IMPORT_OUT_OF_SCOPE"]+=1
            elif exp_reason in ("selector_root_not_an_import","not_a_direct_package_selector"): cls["CORRECT_METHOD_OR_VALUE_SELECTOR_OUT_OF_SCOPE"]+=1
            else: cls["CORRECT_NO_IMPORTED_PACKAGE_CANDIDATE"]+=1
        else:
            if got in ("single_candidate","multiple_candidates") and exp_out=="no_candidate":
                if exp_reason=="import_root_shadowed_by_local_binding": cls["IMPORT_ROOT_SHADOWING_ERROR"]+=1
                else: cls["FALSE_IMPORTED_PACKAGE_FUNCTION_CANDIDATE"]+=1
            elif got in ("single_candidate","multiple_candidates") and exp_out=="out_of_scope":
                cls["METHOD_MISCLASSIFIED_AS_PACKAGE_CALL" if exp_reason in ("selector_root_not_an_import","not_a_direct_package_selector") else "FALSE_IMPORTED_PACKAGE_FUNCTION_CANDIDATE"]+=1
            elif got=="no_candidate" and exp_out in ("single_candidate","multiple_candidates"):
                cls["MISSING_IMPORTED_PACKAGE_FUNCTION_CANDIDATE"]+=1
            else:
                cls["OTHER_MISMATCH"]+=1
            mism.append((rel,w,f"got={got}:{got_reason} exp={exp_out}:{exp_reason}"))

print("\n=== TASK 4D imported package-qualified audit ===")
print("member_selector calls audited:", total)
print("repo-local import-root calls:", repo_local)
for k,v in cls.most_common(): print(f"  {k:48} {v}")
print("\noutcome:reason:")
for k,v in reasons.most_common(): print(f"  {k:48} {v}")
print("\nexternal-test selector calls:", dict(ext_test))
if mism:
    print("\nsample mismatches:")
    for m in mism[:40]: print("  !", m)
    print("FAIL"); sys.exit(1)
print("\nPASS")
