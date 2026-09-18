#!/usr/bin/env python3
"""TASK 3B independent relationship audit.

This script deliberately does **not** reuse the Rust link rules. It re-derives
each sampled relationship from the raw source text and the filesystem, using a
separate, simpler implementation, and then compares that independent verdict
against what the link artifact claims.

That is the point: a checker that shares code with the thing it checks proves
very little. This one shares nothing.

For every rule it samples:

    20 Exact relationships        (where available)
    10 Ambiguous/Unresolved       (where available)

and classifies each as:

    CORRECT_EXACT
    CORRECT_AMBIGUOUS
    CORRECT_UNRESOLVED
    FALSE_EXACT          <- the most severe defect: a false repository fact
    MISSING_CANDIDATE
    WRONG_CANDIDATE
    OUT_OF_SCOPE_BY_POLICY   (not adjudicated here)

Usage:

    scripts/task3b_audit.py --links <links-dir> --repository <repo> [--json out.json]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

# ---------------------------------------------------------------------------
# Loading
# ---------------------------------------------------------------------------


def load_artifact(links_dir: Path):
    manifest = json.loads((links_dir / "manifest.json").read_text())
    links = [
        json.loads(line)
        for line in (links_dir / "links.jsonl").read_text().splitlines()
        if line.strip()
    ]
    entities = [
        json.loads(line)
        for line in (links_dir / "entities.jsonl").read_text().splitlines()
        if line.strip()
    ]
    return manifest, links, entities


# ---------------------------------------------------------------------------
# Independent source scanning
# ---------------------------------------------------------------------------

RS_MOD = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*(;|\{)", re.M)
RS_USE = re.compile(r"^\s*(?:pub\s+)?use\s+([^;]+);", re.M)
GO_PACKAGE = re.compile(r"^\s*package\s+([A-Za-z_][A-Za-z0-9_]*)", re.M)
GO_MODULE = re.compile(r"^\s*module\s+\"?([^\"\s]+)\"?", re.M)
PHP_NAMESPACE = re.compile(r"^\s*namespace\s+([^;{]+)\s*[;{]", re.M)
PHP_CLASSLIKE = re.compile(
    r"^\s*(?:final\s+|abstract\s+|readonly\s+)*"
    r"(class|interface|trait|enum)\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.M,
)
PY_IMPORT = re.compile(r"^\s*import\s+([A-Za-z_][\w.]*)", re.M)
PY_FROM = re.compile(r"^\s*from\s+(\.*)([\w.]*)\s+import\s+(.+)$", re.M)


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""


def all_files(repo: Path):
    return [p for p in repo.rglob("*") if p.is_file()]


def rel(repo: Path, path: Path) -> str:
    return path.relative_to(repo).as_posix()


def python_module_to_paths(module: str):
    parts = module.split(".")
    return ["/".join(parts) + ".py", "/".join(parts) + "/__init__.py"]


def rust_module_dir(relative: str) -> str:
    """Directory in which a Rust module file's children live."""
    parent, _, name = relative.rpartition("/")
    if name in ("mod.rs", "lib.rs", "main.rs"):
        return parent
    stem = name[:-3] if name.endswith(".rs") else name
    return f"{parent}/{stem}" if parent else stem


# ---------------------------------------------------------------------------
# Independent checks, one per rule
# ---------------------------------------------------------------------------


class Repo:
    """An independent, lazily-scanned view of the repository."""

    def __init__(self, root: Path):
        self.root = root
        self._files = None
        self._text = {}

    @property
    def files(self):
        if self._files is None:
            self._files = [rel(self.root, p) for p in all_files(self.root)]
        return self._files

    def text(self, relative: str) -> str:
        if relative not in self._text:
            self._text[relative] = read_text(self.root / relative)
        return self._text[relative]

    def exists(self, relative: str) -> bool:
        return (self.root / relative).is_file()

    # -- Rust ---------------------------------------------------------------

    def rust_module_candidates(self, declaring_file: str, name: str):
        """The two standard source-file forms for `mod name;`."""
        directory = rust_module_dir(declaring_file)
        first = f"{directory}/{name}.rs" if directory else f"{name}.rs"
        second = f"{directory}/{name}/mod.rs" if directory else f"{name}/mod.rs"
        return [p for p in (first, second) if self.exists(p)]

    def rust_declarations_in(self, relative: str):
        """Declaration names written at the top level of a Rust file.

        Deliberately coarse: it over-approximates (nested items are included),
        which is the safe direction for adjudicating an `Exact` claim.
        """
        text = self.text(relative)
        names = set()
        for pattern in (
            r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)",
            r"\bstruct\s+([A-Za-z_][A-Za-z0-9_]*)",
            r"\benum\s+([A-Za-z_][A-Za-z0-9_]*)",
            r"\btrait\s+([A-Za-z_][A-Za-z0-9_]*)",
            r"\btype\s+([A-Za-z_][A-Za-z0-9_]*)",
            r"\bconst\s+([A-Za-z_][A-Za-z0-9_]*)",
            r"\bstatic\s+([A-Za-z_][A-Za-z0-9_]*)",
            r"\bmod\s+([A-Za-z_][A-Za-z0-9_]*)",
            r"\bmacro_rules!\s*([A-Za-z_][A-Za-z0-9_]*)",
        ):
            names.update(re.findall(pattern, text))
        return names

    # -- Go -----------------------------------------------------------------

    def go_packages(self):
        """directory -> set of declared package names."""
        packages = defaultdict(set)
        for relative in self.files:
            if not relative.endswith(".go"):
                continue
            match = GO_PACKAGE.search(self.text(relative))
            if match:
                directory = relative.rpartition("/")[0]
                packages[directory].add(match.group(1))
        return packages

    def go_module_path(self):
        if not self.exists("go.mod"):
            return None
        match = GO_MODULE.search(self.text("go.mod"))
        return match.group(1) if match else None

    # -- PHP ----------------------------------------------------------------

    def php_qualified_names(self):
        """qualified name -> list of (file, kind, name)."""
        found = defaultdict(list)
        for relative in self.files:
            if not relative.endswith(".php"):
                continue
            text = self.text(relative)
            namespaces = PHP_NAMESPACE.findall(text)
            # A file's namespace applies to everything after it; for the audit a
            # single-namespace-per-file reading is enough and is the common case.
            namespace = namespaces[0].strip() if namespaces else None
            if namespace:
                found[namespace].append((relative, "namespace", namespace))
            for kind, name in PHP_CLASSLIKE.findall(text):
                qualified = f"{namespace}\\{name}" if namespace else name
                found[qualified].append((relative, kind, name))
        return found

    # -- Python -------------------------------------------------------------

    def python_imports(self, relative: str):
        """(levels, module, items) for every import in a Python file."""
        text = self.text(relative)
        out = []
        for dots, module, tail in PY_FROM.findall(text):
            items = [part.strip().split(" as ")[0].strip() for part in tail.split(",")]
            out.append((len(dots), module or None, [i for i in items if i]))
        for module in PY_IMPORT.findall(text):
            out.append((0, module, []))
        return out


# ---------------------------------------------------------------------------
# Adjudication
# ---------------------------------------------------------------------------


def source_relative(link):
    return link["source"]["relative_path"].split("#")[0]


def exact_target(link):
    outcome = link["outcome"]
    return outcome.get("target") if outcome["outcome"] == "exact" else None


def target_path(target):
    if target is None:
        return None
    return target.get("relative_path")


def target_decl_name(target):
    if target is None or target.get("kind") != "declaration":
        return None
    return target.get("name")


def adjudicate(repo: Repo, link):
    """Return (verdict, note)."""
    rule = link["rule_id"]
    outcome = link["outcome"]["outcome"]
    written = link["written"]
    source = source_relative(link)

    if rule == "rust.mod.standard_file":
        name = written
        candidates = repo.rust_module_candidates(source, name)
        if outcome == "exact":
            if len(candidates) != 1:
                return "FALSE_EXACT", f"independent candidate set is {candidates}"
            if target_path(exact_target(link)) != candidates[0]:
                return "WRONG_CANDIDATE", f"expected {candidates[0]}"
            return "CORRECT_EXACT", ""
        if outcome == "ambiguous":
            if len(candidates) < 2:
                return "MISSING_CANDIDATE", f"independent candidate set is {candidates}"
            return "CORRECT_AMBIGUOUS", ""
        if outcome == "unresolved":
            if candidates:
                return "MISSING_CANDIDATE", f"independent candidate set is {candidates}"
            return "CORRECT_UNRESOLVED", ""
        return "OUT_OF_SCOPE_BY_POLICY", ""

    if rule == "rust.use.crate_path":
        # Independent module-path derivation. `crate::` is relative to the crate
        # root, which is wherever the owning `lib.rs`/`main.rs` lives, so the
        # module segments are matched against the *tail* of the target path
        # rather than against a fixed prefix.
        segments = written.split("::")
        if segments[0] != "crate":
            return "OUT_OF_SCOPE_BY_POLICY", ""
        if outcome == "exact":
            target = exact_target(link)
            if target is None:
                return "FALSE_EXACT", "no target"
            path = target_path(target)
            kind = target.get("kind")
            tail = segments[1:]
            if not tail:
                return "WRONG_CANDIDATE", "empty module path"
            if kind == "file":
                # `use crate::a::b::*;` names the module itself.
                module_tail = tail[:-1] if tail[-1] == "*" else tail
            elif kind == "declaration":
                module_tail = tail[:-1]
                name = target_decl_name(target)
                if name != tail[-1]:
                    return "WRONG_CANDIDATE", f"declaration {name} != {tail[-1]}"
                if name not in repo.rust_declarations_in(path):
                    return "FALSE_EXACT", f"{name} not found in {path}"
            else:
                return "WRONG_CANDIDATE", f"unexpected target kind {kind}"

            def ends_with_boundary(path: str, suffix: str) -> bool:
                return path == suffix or path.endswith("/" + suffix)

            def file_backed(prefix) -> bool:
                if not prefix:
                    return ends_with_boundary(path, "lib.rs") or ends_with_boundary(
                        path, "main.rs"
                    )
                stem = "/".join(prefix)
                return ends_with_boundary(path, stem + ".rs") or ends_with_boundary(
                    path, stem + "/mod.rs"
                )

            # The longest leading run of module segments that a real file backs.
            # Anything after it must be an *inline* module declared in that file,
            # which is how `mod tests { .. }` inside `linked_list.rs` works.
            backed = None
            for index in range(len(module_tail), -1, -1):
                if file_backed(module_tail[:index]):
                    backed = index
                    break
            if backed is None:
                return "WRONG_CANDIDATE", f"{path} is not under module {module_tail}"
            remaining = module_tail[backed:]
            if remaining:
                inline = set(
                    re.findall(r"\bmod\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{", repo.text(path))
                )
                if not set(remaining) <= inline:
                    return "WRONG_CANDIDATE", (
                        f"{path} does not declare inline module(s) {remaining}"
                    )
            return "CORRECT_EXACT", ""
        if outcome == "unresolved":
            # A conservative check: flag only when the written module path maps
            # cleanly onto an existing file that also declares the name.
            return "CORRECT_UNRESOLVED", "not re-derivable without the module tree"
        if outcome == "ambiguous":
            return "CORRECT_AMBIGUOUS", ""
        return "OUT_OF_SCOPE_BY_POLICY", ""

    if rule == "go.package.same_directory":
        text = repo.text(source)
        match = GO_PACKAGE.search(text)
        if not match or match.group(1) != written:
            return "WRONG_CANDIDATE", "package line does not match"
        if outcome == "exact":
            target = exact_target(link)
            if target is None or target.get("kind") != "structure":
                return "FALSE_EXACT", "no structure target"
            key = target["key"]
            directory = source.rpartition("/")[0]
            if key != f"{directory}:{written}":
                return "WRONG_CANDIDATE", f"entity key {key}"
            return "CORRECT_EXACT", ""
        return "OUT_OF_SCOPE_BY_POLICY", ""

    if rule == "go.import.local_module":
        module = repo.go_module_path()
        if module is None:
            return "FALSE_EXACT" if outcome == "exact" else "CORRECT_UNRESOLVED", "no go.mod"
        if written != module and not written.startswith(module + "/"):
            return "FALSE_EXACT", f"{written} is outside {module}"
        remainder = "" if written == module else written[len(module) + 1 :]
        packages = repo.go_packages().get(remainder, set())
        if outcome == "exact":
            if len(packages) != 1:
                return "FALSE_EXACT", f"independent package set is {sorted(packages)}"
            target = exact_target(link)
            if target is None or target.get("key") != f"{remainder}:{sorted(packages)[0]}":
                return "WRONG_CANDIDATE", f"expected {remainder}:{sorted(packages)[0]}"
            return "CORRECT_EXACT", ""
        if outcome == "ambiguous":
            if len(packages) < 2:
                return "MISSING_CANDIDATE", f"independent package set is {sorted(packages)}"
            return "CORRECT_AMBIGUOUS", ""
        if outcome == "unresolved":
            if packages:
                return "MISSING_CANDIDATE", f"independent package set is {sorted(packages)}"
            return "CORRECT_UNRESOLVED", ""
        return "OUT_OF_SCOPE_BY_POLICY", ""

    if rule in ("go.import.external",):
        module = repo.go_module_path()
        inside = module is not None and (written == module or written.startswith(module + "/"))
        if inside:
            return "FALSE_EXACT", f"{written} is inside {module} but was called external"
        return "OUT_OF_SCOPE_BY_POLICY", ""

    if rule == "python.relative_import.package_path":
        directory = source.rpartition("/")[0]
        dots = len(written) - len(written.lstrip("."))
        tail = written.lstrip(".")
        if not directory:
            return (
                "CORRECT_UNRESOLVED"
                if outcome == "unresolved"
                else "FALSE_EXACT",
                "not inside a package",
            )
        segments = directory.split("/")
        drop = dots - 1
        if drop >= len(segments):
            return (
                "CORRECT_UNRESOLVED" if outcome == "unresolved" else "FALSE_EXACT",
                "escapes the repository root",
            )
        base = segments[: len(segments) - drop]
        base_package = ".".join(base)

        # `from . import name` and `from .name import x` render the same `written`
        # form, so the raw import form is read from the provenance, which carries
        # it as a plain syntax fact rather than a verdict.
        evidence = link["provenance"].get("evidence", [])
        module_segment = None
        for entry in evidence:
            if entry.startswith("written_module="):
                value = entry[len("written_module=") :]
                module_segment = None if value == "<none>" else value

        if module_segment is None:
            # A submodule of the base package, or a package attribute.
            target_module = f"{base_package}.{tail}"
            paths = [p for p in python_module_to_paths(target_module) if repo.exists(p)]
            attribute_holders = [
                relative
                for relative in (f"{'/'.join(base)}/__init__.py",)
                if repo.exists(relative)
            ]
            attributes = set()
            for holder in attribute_holders:
                text = repo.text(holder)
                for pattern in (
                    r"^\s*(?:async\s+)?def\s+([A-Za-z_]\w*)",
                    r"^\s*class\s+([A-Za-z_]\w*)",
                    r"^\s*([A-Za-z_]\w*)\s*=",
                ):
                    attributes.update(re.findall(pattern, text, re.M))
            if tail in attributes and not paths:
                return (
                    "CORRECT_EXACT" if outcome == "exact" else "MISSING_CANDIDATE",
                    "resolves to a package attribute",
                )
            candidates = paths
        else:
            target_module = f"{base_package}.{module_segment}"
            candidates = [p for p in python_module_to_paths(target_module) if repo.exists(p)]

        if outcome == "exact":
            if len(candidates) != 1:
                return "FALSE_EXACT", f"independent candidates are {candidates}"
            if target_path(exact_target(link)) != candidates[0]:
                return "WRONG_CANDIDATE", f"expected {candidates[0]}"
            return "CORRECT_EXACT", ""
        if outcome == "ambiguous":
            if len(candidates) < 2:
                return "MISSING_CANDIDATE", f"independent candidates are {candidates}"
            return "CORRECT_AMBIGUOUS", ""
        if outcome == "unresolved":
            if candidates:
                return "MISSING_CANDIDATE", f"independent candidates are {candidates}"
            return "CORRECT_UNRESOLVED", ""
        return "OUT_OF_SCOPE_BY_POLICY", ""

    if rule == "python.absolute_import.local_candidate":
        # The rule must never be exact, and every candidate must exist.
        if outcome == "exact":
            return "FALSE_EXACT", "absolute imports must never be exact"
        candidates = link["outcome"].get("candidates", [])
        if not candidates:
            return "MISSING_CANDIDATE", "ambiguous with no candidates"
        for candidate in candidates:
            path = target_path(candidate)
            if path is not None and not repo.exists(path):
                return "WRONG_CANDIDATE", f"{path} does not exist"
        return "CORRECT_AMBIGUOUS", ""

    if rule == "python.absolute_import.external":
        # Every local module path derived from the written path must be absent.
        paths = [p for p in python_module_to_paths(written) if repo.exists(p)]
        if paths:
            return "MISSING_CANDIDATE", f"local modules exist: {paths}"
        return "OUT_OF_SCOPE_BY_POLICY", ""

    if rule == "php.namespace.declaration":
        if outcome != "exact":
            return "OUT_OF_SCOPE_BY_POLICY", ""
        text = repo.text(source)
        namespaces = [n.strip() for n in PHP_NAMESPACE.findall(text)]
        if not namespaces:
            return "FALSE_EXACT", "file declares no namespace"
        namespace = namespaces[0]
        if not written.startswith(namespace):
            return "FALSE_EXACT", f"{written} is not under {namespace}"
        leaf = written[len(namespace) :].lstrip("\\")
        if leaf and leaf not in text:
            return "FALSE_EXACT", f"{leaf} not declared in {source}"
        return "CORRECT_EXACT", ""

    if rule == "php.use.qualified_name":
        qualified = repo.php_qualified_names()
        matches = qualified.get(written, [])
        if outcome == "exact":
            if len(matches) != 1:
                return "FALSE_EXACT", f"independent matches are {matches}"
            target = exact_target(link)
            if target_path(target) != matches[0][0]:
                return "WRONG_CANDIDATE", f"expected {matches[0][0]}"
            return "CORRECT_EXACT", ""
        if outcome == "ambiguous":
            if len(matches) < 2:
                return "MISSING_CANDIDATE", f"independent matches are {matches}"
            return "CORRECT_AMBIGUOUS", ""
        if outcome == "unresolved":
            if matches:
                return "MISSING_CANDIDATE", f"independent matches are {matches}"
            return "CORRECT_UNRESOLVED", ""
        return "OUT_OF_SCOPE_BY_POLICY", ""

    return "OUT_OF_SCOPE_BY_POLICY", ""


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------

SEVERE = {"FALSE_EXACT"}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--links", required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--exact-samples", type=int, default=20)
    parser.add_argument("--other-samples", type=int, default=10)
    parser.add_argument("--json", default=None)
    args = parser.parse_args()

    links_dir = Path(args.links)
    repo = Repo(Path(args.repository))
    manifest, links, entities = load_artifact(links_dir)

    print(f"links artifact : {links_dir}")
    print(f"repository     : {repo.root}")
    print(f"snapshot       : {manifest['snapshot_digest']}")
    print(f"link digest    : {manifest['link_digest']}")
    print(f"links          : {len(links)}")
    print(f"entities       : {len(entities)}")
    outcomes = manifest["outcomes"]
    print(
        "outcomes       : "
        f"exact {outcomes['exact']} ambiguous {outcomes['ambiguous']} "
        f"unresolved {outcomes['unresolved']} out_of_scope {outcomes['out_of_scope']}"
    )
    print()

    by_rule = defaultdict(list)
    for link in links:
        by_rule[link["rule_id"]].append(link)

    print("counts by rule:")
    for rule_id in sorted(by_rule):
        print(f"  {rule_id:<46} {len(by_rule[rule_id])}")
    print()

    verdicts = Counter()
    severe = []
    samples = []

    for rule_id in sorted(by_rule):
        exact = [link for link in by_rule[rule_id] if link["outcome"]["outcome"] == "exact"]
        other = [link for link in by_rule[rule_id] if link["outcome"]["outcome"] != "exact"]
        selected = exact[: args.exact_samples] + other[: args.other_samples]
        print(f"--- {rule_id}: sampling {len(selected)} of {len(by_rule[rule_id])}")
        for link in selected:
            verdict, note = adjudicate(repo, link)
            verdicts[verdict] += 1
            if verdict in SEVERE:
                severe.append((rule_id, link, note))
            samples.append(
                {
                    "rule_id": rule_id,
                    "source": link["source"],
                    "written": link["written"],
                    "outcome": link["outcome"],
                    "provenance": link["provenance"],
                    "verdict": verdict,
                    "note": note,
                }
            )
            marker = "!!" if verdict in SEVERE else "  "
            print(
                f"  {marker} {verdict:<20} {link['outcome']['outcome']:<12} "
                f"{source_relative(link)}  {link['written']}"
            )
            if note:
                print(f"       note: {note}")
        print()

    print("=" * 78)
    print("verdict totals:")
    for verdict, count in sorted(verdicts.items()):
        print(f"  {verdict:<24} {count}")
    print()
    print(f"FALSE_EXACT count: {len(severe)}")
    for rule_id, link, note in severe:
        print(f"  {rule_id}: {source_relative(link)} {link['written']} -- {note}")

    if args.json:
        Path(args.json).write_text(
            json.dumps(
                {
                    "links_dir": str(links_dir),
                    "repository": str(repo.root),
                    "snapshot_digest": manifest["snapshot_digest"],
                    "link_digest": manifest["link_digest"],
                    "links": len(links),
                    "entities": len(entities),
                    "outcomes": outcomes,
                    "counts_by_rule": {k: len(v) for k, v in sorted(by_rule.items())},
                    "verdicts": dict(verdicts),
                    "false_exact": len(severe),
                    "samples": samples,
                },
                indent=2,
            )
        )
        print(f"\nwrote {args.json}")

    return 1 if severe else 0


if __name__ == "__main__":
    sys.exit(main())
