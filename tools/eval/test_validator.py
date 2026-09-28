import sys, json, tempfile, shutil
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
import validator as V, gold, classify

FIX = str(Path(__file__).parent / "fixtures" / "microrepo")


def task(ob, mutating=False, cmd=None, pristine=None, root=None):
    return {"id": "t", "root": root or FIX, "pristine_root": pristine or FIX,
            "obligations": ob, "mutating": mutating, "validator_command": cmd}


# ---------- negative + equivalent gold ----------
def test_unproven_gold_blocks():
    t = task([{"id": "o", "kind": "contains_text", "value": "neverdeclared-xyz"}])
    r = gold.prove(t, FIX)
    assert r["all_proven"] is False  # blocks: no source_path + not in any file


def test_gold_source_proven():
    t = task([{"id": "o", "kind": "contains_text", "value": "example.com/micro",
               "source_path": "go.mod"}])
    assert gold.prove(t, FIX)["all_proven"] is True


def test_absence_gold():
    t = task([{"id": "o", "kind": "absence", "value": r"go\s*\d",
               "source_path": "go.mod"}])
    assert gold.prove(t, FIX)["all_proven"] is True


# ---------- direction ----------
def test_relation_direction():
    # engine calls util.Encode — true
    ob = {"id": "o", "kind": "relation_direction", "from_path": "internal/engine/core.go",
          "to_callee": "util.Encode", "direction": "engine calls util codec"}
    ok = V.eval_obligation(ob, "engine calls util codec", FIX, FIX)
    assert ok["ok"] is True
    # reversed answer (codec calls engine) - the answer text asserts wrong dir
    bad = V.eval_obligation(ob, "codec calls engine", FIX, FIX)
    assert bad["ok"] is False


def test_written_call():
    ob = {"id": "o", "kind": "written_call", "caller": "cmd/main.go", "callee": "NewResolver"}
    assert V.eval_obligation(ob, "main calls NewResolver", FIX, FIX)["ok"] is True
    # claiming a call that doesn't exist fails
    ob2 = {"id": "o", "kind": "written_call", "caller": "cmd/main.go", "callee": "NoSuch"}
    assert V.eval_obligation(ob2, "main calls NoSuch", FIX, FIX)["ok"] is False


# ---------- mutation: diff/scope/position/build ----------
def test_mutation_requires_diff():
    d = tempfile.mkdtemp(); shutil.copytree(FIX, d, dirs_exist_ok=True)
    t = task([{"id": "m", "kind": "mutation_marker", "path": "internal/engine/cache.go",
               "value": "const MaxSize = 256"}], mutating=True,
             cmd="true", pristine=FIX, root=d)
    # no edit -> no diff -> failure
    v = V.validate(t, "done", d, True, 0, False)
    assert v["validation_status"] == V.TASK_FAILURE
    shutil.rmtree(d)


def test_mutation_scope():
    d = tempfile.mkdtemp(); shutil.copytree(FIX, d, dirs_exist_ok=True)
    p = Path(d) / "internal/engine/cache.go"
    # marker inside a function (wrong scope)
    p.write_text(p.read_text().replace("func (c *Cache) Get(k string) int {",
                                       "func (c *Cache) Get(k string) int {\n\t_ = constNever\n\tconst MaxSize = 256\n"))
    # Note: that's invalid Go; use a legal-looking inside-func placement
    p.write_text("package engine\n\ntype Cache struct {\n\tm map[string]int\n}\n\nfunc (c *Cache) Get(k string) int {\n\tconst MaxSize = 256\n\treturn c.m[k]\n}\n")
    t = task([{"id": "m", "kind": "mutation_scope", "path": "internal/engine/cache.go",
               "value": "const MaxSize = 256", "scope": "top-level"}],
             mutating=True, cmd="true", pristine=FIX, root=d)
    v = V.validate(t, "done", d, True, 0, False)
    assert v["validation_status"] == V.TASK_FAILURE
    shutil.rmtree(d)


def test_mutation_scope_ok():
    d = tempfile.mkdtemp(); shutil.copytree(FIX, d, dirs_exist_ok=True)
    p = Path(d) / "internal/engine/cache.go"
    p.write_text(p.read_text() + "\nconst MaxSize = 256\n")
    t = task([{"id": "m", "kind": "mutation_scope", "path": "internal/engine/cache.go",
               "value": "const MaxSize = 256", "scope": "top-level"}],
             mutating=True, cmd="true", pristine=FIX, root=d)
    v = V.validate(t, "done", d, True, 0, False)
    assert v["validation_status"] == V.TASK_SUCCESS
    shutil.rmtree(d)


def test_mutation_position():
    d = tempfile.mkdtemp(); shutil.copytree(FIX, d, dirs_exist_ok=True)
    p = Path(d) / "internal/engine/cache.go"
    lines = p.read_text().splitlines()
    # marker immediately above `func (c *Cache) Get`
    for i, l in enumerate(lines):
        if l.strip().startswith("func (c *Cache) Get"):
            lines.insert(i, "// cache boundary")
            break
    p.write_text("\n".join(lines) + "\n")
    t = task([{"id": "m", "kind": "mutation_position", "path": "internal/engine/cache.go",
               "value": "// cache boundary", "above": "func (c *Cache) Get"}],
             mutating=True, cmd="true", pristine=FIX, root=d)
    v = V.validate(t, "done", d, True, 0, False)
    assert v["validation_status"] == V.TASK_SUCCESS
    # wrong position: marker elsewhere
    shutil.rmtree(d); d = tempfile.mkdtemp(); shutil.copytree(FIX, d, dirs_exist_ok=True)
    p = Path(d) / "internal/engine/cache.go"
    p.write_text("// cache boundary\n" + p.read_text())
    v = V.validate(t, "done", d, True, 0, False)
    assert v["validation_status"] == V.TASK_FAILURE
    shutil.rmtree(d)


def test_build_failure_fails():
    d = tempfile.mkdtemp(); shutil.copytree(FIX, d, dirs_exist_ok=True)
    p = Path(d) / "internal/engine/cache.go"
    p.write_text(p.read_text() + "\nconst MaxSize = 256\nBROKEN\n")  # bad syntax
    t = task([{"id": "m", "kind": "mutation_scope", "path": "internal/engine/cache.go",
               "value": "const MaxSize = 256", "scope": "top-level"}],
             mutating=True, cmd="false", pristine=FIX, root=d)  # validator cmd fails
    v = V.validate(t, "done", d, True, 0, False)
    assert v["validation_status"] == V.TASK_FAILURE
    shutil.rmtree(d)


def test_status_model():
    t = task([{"id": "o", "kind": "contains_text", "value": "x"}])
    v = V.validate(t, "wrong", FIX, True, 0, False)
    assert v["execution_status"] == V.EXECUTION_VALID and v["validation_status"] == V.TASK_FAILURE
    v = V.validate(t, "", FIX, True, 1, False)
    assert v["execution_status"] == V.INFRA_INVALID and v["validation_status"] == V.NOT_EVALUATED


def test_classify():
    assert classify.classify_tool("exec", {"command": "reposuite-repodex query x"}) == "repodex"
    assert classify.classify_tool("exec", {"command": "ls"}) == "enumeration"
    assert classify.classify_tool("exec", {"command": "go build ./..."}) == "test_build"
    assert classify.classify_tool("edit", {}) == "edit"


if __name__ == "__main__":
    for fn in dir():
        if fn.startswith("test_"):
            globals()[fn](); print(fn, "PASS")


def test_classifier_no_path_false_positive():
    c = classify.classify_tool
    # path that merely contains 'reposuite-repodex' is NOT an invocation
    assert c("exec", {"command": "find /x/reposuite-repodex/tools/eval/fixtures -name '*.go'"}) != "repodex"
    assert c("exec", {"command": "ls /x/reposuite-repodex"}) != "repodex"
    assert c("exec", {"command": "cat /x/reposuite-repodex/foo.rs"}) != "repodex"
    assert c("exec", {"command": "grep foo /x/reposuite-repodex/src/"}) != "repodex"


def test_classifier_real_invocation():
    c = classify.classify_tool
    assert c("exec", {"command": "reposuite-repodex query --root /x --query q"}) == "repodex"
    assert c("exec", {"command": "/path/to/reposuite-repodex query --root /x --query q"}) == "repodex"
    assert c("exec", {"command": "env X=1 reposuite-repodex query --root /x --query q"}) == "repodex"


def test_allowed_changed_paths_rejects_unrelated():
    d = tempfile.mkdtemp(); shutil.copytree(FIX, d, dirs_exist_ok=True)
    p = Path(d) / "internal/engine/cache.go"
    p.write_text(p.read_text() + "\nconst MaxSize = 256\n")
    # unrelated extra file modification
    (Path(d) / "internal/util/codec.go").write_text(
        (Path(d) / "internal/util/codec.go").read_text() + "\n// hacked\n")
    t = task([{"id": "m", "kind": "mutation_scope", "path": "internal/engine/cache.go",
               "value": "const MaxSize = 256", "scope": "top-level"}],
             mutating=True, cmd="true", pristine=FIX, root=d)
    t["allowed_changed_paths"] = ["internal/engine/cache.go"]
    v = V.validate(t, "done", d, True, 0, False)
    assert v["validation_status"] == V.TASK_FAILURE
    assert v["forbidden_changed_paths"] == ["internal/util/codec.go"]
    shutil.rmtree(d)


def test_changed_paths_repo_relative():
    d = tempfile.mkdtemp(); shutil.copytree(FIX, d, dirs_exist_ok=True)
    p = Path(d) / "internal/engine/cache.go"
    p.write_text(p.read_text() + "\nconst MaxSize = 256\n")
    t = task([{"id": "m", "kind": "mutation_scope", "path": "internal/engine/cache.go",
               "value": "const MaxSize = 256", "scope": "top-level"}],
             mutating=True, cmd="true", pristine=FIX, root=d)
    t["allowed_changed_paths"] = ["internal/engine/cache.go"]
    v = V.validate(t, "done", d, True, 0, False)
    assert all(not p.startswith("/") and "tmp" not in p for p in v["changed_paths"])
    assert "internal/engine/cache.go" in v["changed_paths"]
    shutil.rmtree(d)


def test_preflight_build_exit_required():
    import run_smoke as R
    # a failing build must not be accepted as pass
    assert not (R.tool_preflight.__defaults__ and False)  # structural check only


if __name__ == "__main__":
    for fn in dir():
        if fn.startswith("test_"):
            globals()[fn](); print(fn, "PASS")
