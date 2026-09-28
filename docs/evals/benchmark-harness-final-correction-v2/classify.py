"""Tool-call classification from executed content (not just tool name).

Ordinary `exec` is NOT a RepoDex call. Classification inspects the executed
command string.
"""
import re

def classify_tool(function_name, args):
    """Return (category, executed_content_description)."""
    cmd = ""
    if isinstance(args, dict):
        cmd = str(args.get("command", "") or args.get("file_path", "") or args.get("pattern", ""))
    if function_name == "exec":
        if "reposuite-repodex" in cmd or "repodex query" in cmd or "repo_query" in cmd:
            return "repodex"
        if re.search(r"\b(go|cargo|pytest|make|npm|tsc|clang|gcc)\b.*\b(build|test|vet|check|lint)\b", cmd):
            return "test_build"
        if re.search(r"\bgrep|\bfind|\bripgrep|\brg\b", cmd):
            return "native_search"
        if re.search(r"\bfind\b|\bls\b|\bdir\b", cmd):
            return "enumeration"
        if re.search(r"\bcat\b|\bhead\b|\btail\b|\bsed\b|\bawk\b", cmd):
            return "source_read"
        return "other_exec"
    if function_name in ("grep", "find_file_by_name"):
        return "native_search"
    if function_name in ("read", "read_file"):
        return "source_read"
    if function_name in ("edit", "write", "str_replace_editor"):
        return "edit"
    if function_name == "git":
        return "git"
    return function_name or "unknown"


def is_rejected_result(content):
    """Detect approval-layer rejection vs legitimate tool failure."""
    c = str(content).lower()
    return any(p in c for p in (
        "rejected", "not approved", "permission denied",
        "requires approval", "denied by policy", "canceled",
        "cancelled", "requires user approval"))


def exit_code_of(content):
    """Extract a process exit code if the harness embeds it in the result text."""
    import re
    m = re.search(r"(?:exit[ _]?code|exit status|status)[:\s]+(-?\d+)", str(content), re.I)
    if m:
        return int(m.group(1))
    # Heuristic: devin wraps exec output and reports "Exit code: N" — captured above.
    return None
