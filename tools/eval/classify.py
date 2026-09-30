"""Tool-call classification from executed content (not just tool name).

Ordinary `exec` is NOT a RepoDex call. Classification inspects the executed
command string.
"""
import re
import shlex


def _is_repodex_invocation(cmd):
    """True only if the command's executable token is a reposuite-repodex
    binary being run (e.g. `reposuite-repodex query ...`, `.../reposuite-repodex
    query ...`, `env X=1 reposuite-repodex ...`). A path that merely *contains*
    'reposuite-repodex' (find/ls/cat on the repo dir) is NOT an invocation."""
    try:
        toks = shlex.split(cmd)
    except Exception:
        toks = cmd.split()
    if not toks:
        return False
    # skip `env VAR=val` wrappers to reach the real program
    i = 0
    if toks[0] == "env":
        i = 1
        while i < len(toks) and "=" in toks[i] and not toks[i].startswith("-"):
            i += 1
    if i >= len(toks):
        return False
    prog = toks[i].rsplit("/", 1)[-1]
    # the benchmark `repo_query` shim wraps `reposuite-repodex query` with an
    # externally bound frozen root (§19-§23) — count it.
    if prog == "repo_query":
        return True
    # `reposuite dex <sub>` / `dex <sub>` — a RepoDex attempt under the wrong
    # executable name (seen in real traces). Count as repodex usage; the
    # command's failure is visible in the tool result anyway (§38).
    if prog == "dex":
        return len(toks) > i + 1
    if prog == "reposuite" and i + 1 < len(toks) and toks[i + 1] == "dex":
        return len(toks) > i + 2
    # the executable basename must be the reposuite-repodex binary
    if prog != "reposuite-repodex":
        return False
    # must be invoked with a real subcommand (not just the path mentioned)
    return len(toks) > i + 1


def _classify_cmd_one(cmd):
    """Classify ONE shell command segment."""
    if _is_repodex_invocation(cmd):
        return "repodex"
    if re.search(r"\b(go|cargo|pytest|make|npm|tsc|clang|gcc)\b.*\b(build|test|vet|check|lint)\b", cmd):
        return "test_build"
    if re.search(r"\bgrep|\bfind|\bripgrep|\brg\b", cmd):
        return "native_search"
    if re.search(r"\bfind\b|\bls\b|\bdir\b|\btree\b", cmd):
        return "enumeration"
    if re.search(r"\bcat\b|\bhead\b|\btail\b|\bsed\b|\bawk\b", cmd):
        return "source_read"
    if re.search(r"^\s*(cd|export|mkdir|env|which|echo|pwd|true)\b", cmd):
        return "other_exec"  # pure shell plumbing — not a discovery op
    return "other_exec"


def classify_cmd_segments(cmd):
    """§39 operation-level accounting: split a compound exec command on
    `&&` `;` `||` `|` newlines and classify each segment. Returns a list —
    `cd x && grep foo` contributes ['other_exec','native_search'], not just the
    first executable's category."""
    segs = [s for s in re.split(r"&&|\|\||;|\||\n", str(cmd)) if s.strip()]
    if not segs:
        segs = [cmd]
    return [_classify_cmd_one(s) for s in segs]


def classify_tool(function_name, args):
    """Return the PRIMARY category for a tool call (compat: first non-plumbing
    segment wins, else the last segment's class)."""
    return classify_tool_ops(function_name, args)[0]


def classify_tool_ops(function_name, args):
    """Return ALL operation categories for a tool call (§39)."""
    cmd = ""
    if isinstance(args, dict):
        cmd = str(args.get("command", "") or args.get("file_path", "") or args.get("pattern", ""))
    if function_name == "exec":
        return classify_cmd_segments(cmd)
    if function_name in ("grep", "find_file_by_name"):
        return ["native_search"]
    if function_name in ("read", "read_file"):
        return ["source_read"]
    if function_name in ("edit", "write", "str_replace_editor"):
        return ["edit"]
    if function_name == "git":
        return ["git"]
    return [function_name or "unknown"]


def is_rejected_result(content):
    """Detect approval-layer rejection vs legitimate tool failure.

    Match only explicit approval/permission-denial phrases — not the bare word
    'rejected'/'canceled', which can appear inside ordinary tool output (e.g. an
    exported session JSON the agent reads)."""
    c = str(content).lower()
    return any(p in c for p in (
        "not approved", "permission denied", "requires approval",
        "requires user approval", "denied by policy", "auto-denied",
        "auto denied", "tool call rejected", "tool rejected",
        "rejected by policy", "approval required", "was rejected",
        "call was canceled", "tool was canceled", "tool was cancelled"))


def exit_code_of(content):
    """Extract a process exit code if the harness embeds it in the result text."""
    import re
    m = re.search(r"(?:exit[ _]?code|exit status|status)[:\s]+(-?\d+)", str(content), re.I)
    if m:
        return int(m.group(1))
    # Heuristic: devin wraps exec output and reports "Exit code: N" — captured above.
    return None
