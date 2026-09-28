import sys, json, tempfile, shutil
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
import validator as V

def task(ob, mutating=False):
    return {"id": "t", "root": "/x", "pristine_root": "/x",
            "obligations": ob, "mutating": mutating}


def test_contains_negative():
    ob = [{"id": "o", "kind": "contains", "value": "example.com/mempilot"}]
    r = V.eval_obligation(ob[0], "module example.com/mempilot", "/x")
    assert r["ok"] is True
    r = V.eval_obligation(ob[0], "module example.com/wrong", "/x")
    assert r["ok"] is False


def test_absent_claim():
    ob = {"id": "o", "kind": "absent_claim", "value": r"go\s*1\."}
    # correct: reports absence
    ok = V.eval_obligation(ob, "go.mod declares no Go version directive", "/x")
    assert ok["ok"] is True
    # wrong: asserts a version
    bad = V.eval_obligation(ob, "go.mod requires go 1.21", "/x")
    assert bad["ok"] is False


def test_equivalent_correct():
    ob = {"id": "o", "kind": "contains", "value": "serde|toml"}
    for ans in ["uses serde", "depends on toml", "serde is declared"]:
        assert V.eval_obligation(ob, ans, "/x")["ok"] is True


def test_mutating_worktree_proof():
    d = tempfile.mkdtemp()
    try:
        p = Path(d) / "f.go"
        # case 1: marker present -> success
        p.write_text("const MaxSize = 256\nfunc g(){}")
        t = task([{"id": "m", "kind": "marker", "path": "f.go",
                   "value": "const MaxSize = 256"}], mutating=True)
        t["root"] = d; t["pristine_root"] = d
        v = V.validate(t, "done", d, True, 0, False)
        assert v["execution_status"] == V.EXECUTION_VALID
        assert v["validation_status"] == V.TASK_SUCCESS
        # case 2: agent claims done but file unchanged -> failure
        p.write_text("func g(){}")
        v = V.validate(t, "i added the constant", d, True, 0, False)
        assert v["validation_status"] == V.TASK_FAILURE
    finally:
        shutil.rmtree(d)


def test_status_model():
    t = task([{"id": "o", "kind": "contains", "value": "x"}])
    # wrong answer, fully executed
    v = V.validate(t, "wrong answer", "/x", True, 0, False)
    assert v["execution_status"] == V.EXECUTION_VALID and v["validation_status"] == V.TASK_FAILURE
    # rejected tool -> infra invalid
    v = V.validate(t, "", "/x", True, 1, False)
    assert v["execution_status"] == V.INFRA_INVALID and v["validation_status"] == V.NOT_EVALUATED
    # provider failure
    v = V.validate(t, "", "/x", False, 0, False)
    assert v["execution_status"] == V.MODEL_PROVIDER_FAILURE


def test_reproducible():
    t = task([{"id": "o", "kind": "contains", "value": "abc"}])
    a = V.validate(t, "abc yes", "/x", True, 0, False)
    b = V.validate(t, "abc yes", "/x", True, 0, False)
    assert a == b


def test_classify():
    from classify import classify_tool
    assert classify_tool("exec", {"command": "reposuite-repodex query ..."}) == "repodex"
    assert classify_tool("exec", {"command": "ls -la"}) == "enumeration"
    assert classify_tool("exec", {"command": "grep -rn foo"}) == "native_search"
    assert classify_tool("exec", {"command": "go build ./..."}) == "test_build"
    assert classify_tool("read", {"file_path": "x"}) == "source_read"
    assert classify_tool("edit", {}) == "edit"


if __name__ == "__main__":
    for fn in dir():
        if fn.startswith("test_"):
            globals()[fn](); print(fn, "PASS")
