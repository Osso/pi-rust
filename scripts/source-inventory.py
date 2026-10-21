#!/usr/bin/env python3
"""Validate locked source selection; optionally regenerate the re-pin matrix."""
import argparse
from collections import Counter
import json
from pathlib import Path
import re
import subprocess

SOURCE_EXTENSIONS = {".ts", ".js", ".mjs", ".cjs", ".c", ".sh", ".sql", ".css", ".html"}
OTHER_EXTENSIONS = {".md", ".json", ".jsonl", ".service", "", ".png", ".node", ".wasm"}
STATUSES = {"todo", "partial", "done", "gap"}
REMOVED = "## Removed by re-pin (old pin only)"


def git(root, *args):
    return subprocess.run(["git", "--no-optional-locks", "-C", str(root), *args],
                          check=True, capture_output=True).stdout


def select_sources(files):
    expected, excluded = {}, Counter()
    for name, data in files.items():
        path = Path(name)
        if (set(path.parts) & {"test", "tests", "__tests__"}
                or re.search(r"\.(test|spec)\.", path.name)
                or name == "packages/coding-agent/examples/extensions/custom-provider-gitlab-duo/test.ts"):
            excluded["test"] += 1
        elif path.suffix in SOURCE_EXTENSIONS or ("/src/" in name and path.suffix == ".json"):
            expected[name.removeprefix("packages/")] = data.count(b"\n") + int(bool(data) and not data.endswith(b"\n"))
        else:
            if path.suffix not in OTHER_EXTENSIONS:
                raise ValueError(f"Unclassified source kind: {name}")
            excluded[path.suffix or "extensionless"] += 1
    return expected, excluded


def parse_rows(text):
    rows = []
    for line in text.splitlines():
        if re.match(r"^\| `[^`]+` \| \d+ \|", line):
            fields = line.strip("| ").split(" | ", 5)
            assert len(fields) == 6, f"Invalid row: {line}"
            fields[0] = fields[0].strip("`")
            rows.append(fields)
    return rows


def repin_rows(old, expected, changes):
    old = {name: row.copy() for name, row in old.items()}
    for row in old.values():
        if not (row[2].isdigit() or row[2] == "—"):
            assert row[2].startswith("`"), f"Invalid legacy level: {row[0]}"
            row[2], row[4], row[5] = "—", row[2], row[4] + "; " + row[5]
    changed, renamed = {}, {}
    for kind, paths in changes:
        paths = [p.removeprefix("packages/") for p in paths]
        changed[paths[-1]] = kind
        if kind == "R":
            renamed[paths[-1]] = paths[0]
    rows = []
    for name, lines in sorted(expected.items()):
        previous = old.get(renamed.get(name, name))
        kind = changed.get(name)
        if kind == "A":
            row = [name, str(lines), "—", "todo", "—", "new on master"]
        elif previous is not None:
            row = previous.copy()
            row[:2] = [name, str(lines)]
            if kind in {"M", "R"}:
                row[3] = "todo"
                origin = f"; old path: `{previous[0]}`" if kind == "R" else ""
                row[5] = f"re-pin delta; old status: {previous[3]}; old Rust: {previous[4]}{origin}; {previous[5]}"
            else:
                assert kind is None, (name, kind)
                assert int(previous[1]) == lines, f"Unclassified line delta: {name}"
                row[5] += "; re-run vs 4f9e6f8a4 pending"
        else:
            raise ValueError(f"Unclassified new source: {name}")
        rows.append(row)
    renamed_old = set(renamed.values())
    removed = [row for name, row in sorted(old.items()) if name not in expected and name not in renamed_old]
    return rows, removed


def render(rows, removed):
    total = sum(int(row[1]) for row in rows)
    counts = Counter(row[3] for row in rows)
    text = ["# Port coverage matrix", "",
            f"Source: `packages/` at [source-lock.json](source-lock.json): **{len(rows):,} unique files, {total:,} lines**.",
            "Selection and validation: [inventory](inventory.md). Status counts: " + ", ".join(f"`{s}` {counts[s]}" for s in sorted(STATUSES)) + ".", "",
            "Only byte-identical rows retain old labels/mappings; those proofs still require re-run. Modified/renamed rows are `todo`; old approvals remain pending revalidation. No Rust code deleted by this inventory.", "",
            "Level is the old import-graph depth where available, not recomputed for changed modules."]
    for package in sorted({row[0].split("/")[0] for row in rows}):
        group = [row for row in rows if row[0].startswith(package + "/")]
        text += ["", f"## {package} ({len(group)} files, {sum(int(row[1]) for row in group)} lines)", "",
                 "| Source | Lines | Level | Status | Rust | Notes |", "|---|---:|---:|---|---|---|"]
        text += ["| `" + row[0] + "` | " + " | ".join(row[1:]) + " |" for row in group]
    text += ["", REMOVED, "", f"{len(removed)} old rows; not part of active totals. Rust deletion is deferred to product slices.", "",
             "| Old source | Old status | Rust crate/module to delete |", "|---|---|---|"]
    text += [f"| `{row[0]}` | {row[3]} | {row[4]} |" for row in removed]
    return "\n".join(text) + "\n"


def validate(text, expected):
    active = text.split(REMOVED)[0]
    rows = parse_rows(active)
    actual = {row[0]: int(row[1]) for row in rows}
    assert len(rows) == len(actual), "Duplicate matrix rows"
    assert actual == expected, {"missing": sorted(expected.keys() - actual.keys()),
                                "extra": sorted(actual.keys() - expected.keys()),
                                "counts": [n for n in actual.keys() & expected.keys() if actual[n] != expected[n]]}
    assert all(row[3] in STATUSES for row in rows), "Invalid status"
    assert all(row[2].isdigit() or row[2] == "—" for row in rows), "Invalid level"
    counts = Counter(row[3] for row in rows)
    summary = re.search(r"Status counts: (.+)\.", active).group(1)
    displayed = dict((status, int(count)) for status, count in re.findall(r"`(\w+)` (\d+)", summary))
    assert displayed == {status: counts[status] for status in STATUSES}, "Status summary mismatch"
    count, lines = re.search(r"\*\*([\d,]+) unique files, ([\d,]+) lines\*\*", active).groups()
    assert len(actual) == int(count.replace(",", "")), "Header file count"
    assert sum(actual.values()) == int(lines.replace(",", "")), "Header line count"
    sections = re.split(r"^## ([\w-]+) \((\d+) files, (\d+) lines\)\n", active, flags=re.M)
    assert (len(sections) - 1) // 4 == len({name.split('/')[0] for name in expected})
    for index in range(1, len(sections), 4):
        package, count, lines, body = sections[index:index + 4]
        group = parse_rows(body)
        assert all(row[0].startswith(package + "/") for row in group)
        assert len(group) == int(count)
        assert sum(int(row[1]) for row in group) == int(lines)
    return Counter(row[3] for row in rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="regenerate using the recorded pre-repin matrix and classification")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    lock = json.loads((root / "docs/porting/source-lock.json").read_text())
    source = Path(lock["worktree"])
    assert git(source, "rev-parse", "HEAD").decode().strip() == lock["commit"]
    git(source, "diff", "--exit-code", lock["commit"], "--", "packages")
    names = git(source, "ls-tree", "-r", "--name-only", lock["commit"], "--", "packages").decode().splitlines()
    expected, excluded = select_sources({name: (source / name).read_bytes() for name in names})
    matrix = root / "docs/porting/coverage.md"
    if args.write:
        old_text = git(root, "show", "8035c95a:docs/porting/coverage.md").decode()
        old = {row[0]: row for row in parse_rows(old_text)}
        changes = json.loads((root / "docs/porting/repin-classification.json").read_text())
        matrix.write_text(render(*repin_rows(old, expected, changes)))
    counts = validate(matrix.read_text(), expected)
    print(f"PASS: {len(expected)} unique rows, {sum(expected.values())} lines; zero missing/extra/duplicate/count mismatches")
    print("Statuses:", dict(sorted(counts.items())))
    print("Excluded non-source/test categories:", dict(sorted(excluded.items())))


if __name__ == "__main__":
    main()
