# Source inventory reconciliation

Pinned source: [source-lock.json](source-lock.json). Matrix: [coverage.md](coverage.md).

## Scope and counting

Inventory covers all ten directories under `packages/`, including nested SQLite, bundled extensions, examples, tooling, native addons, and vendored sources. Select tracked `.ts` (including `.d.ts`), `.js`, `.mjs`, `.cjs`, `.c`, `.sh`, `.sql`, `.css`, `.html`, plus `.json` anywhere under `src/`. Do not restrict discovery to top-level package `src/` directories. Public `testing/` conformance helpers are source, not test suites.

Exclude directories named `test`, `tests`, `__tests__`, filenames containing `.test.` or `.spec.`, and the standalone GitLab Duo example `test.ts` (its header identifies a test driver). Runnable extension demonstrations such as `overlay-test.ts` remain inventoried; test-runner configuration is tooling source. Generated/vendored tracked source is included; ignored generated provider data is not part of the pinned Git tree.

Line count = LF bytes plus one for a nonempty unterminated final line. Never count binary newline bytes as source lines. Other tracked files are metadata/configuration/data/docs or binary assets, not text-source rows: `.md`, `.json` outside `src/`, `.jsonl`, `.service`, extensionless dotfiles, `.png`, `.node`, `.wasm`. Validator reports these categories and rejects unknown extensions rather than silently excluding them. Binary runtime assets remain outside line totals: `coding-agent/src/modes/interactive/assets/clankolas.png`, example DOOM `doom/build/doom.wasm`, and four `tui/native/{darwin,win32}/prebuilds/**/*.node` files. Native C and DOOM JS/C/build sources are included. Binary asset parity is not established by this audit; PNGs under docs/test are documentation/test fixtures.

## Reconciliation evidence (2026-10-20)

Old header: 671 files / 164,076 lines. Actual old matrix: 675 unique rows / 164,543 recorded lines. `coding-agent/src/core/provider-composer.ts` has 572 lines, not the recorded 573. All other old row counts match pinned bytes.

Eight missing TypeScript modules/declarations, relative to `packages/`:

| Source | Lines |
|---|---:|
| `agent/src/harness/session/testing/conformance.ts` | 1016 |
| `agent/src/harness/session/testing/index.ts` | 6 |
| `agent/src/harness/session/testing/types.ts` | 16 |
| `ai/src/providers/data-json.d.ts` | 4 |
| `coding-agent/src/utils/highlight-js-lib-index.d.ts` | 19 |
| `telemetry/src/testing/conformance.ts` | 315 |
| `telemetry/src/testing/index.ts` | 6 |
| `telemetry/src/testing/types.ts` | 18 |

These add 1,400 lines. Corrected legacy TS selection: **683 files / 165,942 lines**. Full selection adds **136 further sources / 25,757 lines**, yielding **819 unique rows / 191,699 lines**. Those additional paths are explicit rows, including non-TS runtime modules and outside-`src/` package examples/tooling; eight was not the total all-language/all-package omission count.

At reconciliation commit `bf639a5`, all 144 added rows were `todo`: unresolved module-level parity, not approved exclusions and not a claim that no corresponding Rust implementation exists. Existing 670 `done` and five `gap` labels are retained, not independently re-proven. No approval added: debug REPL was already approved on 2026-10-18 and already has [traceability](traceability/debug-repl.md); only its omission from `AGENTS.md` is repaired. Giant Rust module readability remains structural debt, untouched here.

## Authorized v15 addition

The v15 slice adds `coding-agent/extensions/session-autoname/src/index.ts`, absent from the original pin: 295 lines at the revision in [source-lock.json](source-lock.json). Its `additionalSourceFiles` list extends the inventory without changing the original pin or the counts of existing pinned modules. Matrix totals: **820 unique files / 191,994 lines**. Current statuses live only in [coverage.md](coverage.md).

## Repeatable validation

No existing source-inventory/manifest validator was found in the Rust checkout or pinned source. Docs-only slice adds no executable. Run the following Python program from the Rust checkout (for example, supply it to `uv run --no-project python -`). It uses only Python's standard library and read-only Git commands. It checks pinned identity/content, selection completeness, duplicate rows, every line count, package subtotals, and header totals; it does not verify port behavior.

```python
import collections
import json
from pathlib import Path
import re
import subprocess

root = Path.cwd()
lock = json.loads((root / "docs/porting/source-lock.json").read_text())
source = Path(lock["worktree"])

def git(*args):
    return subprocess.run(
        ["git", "-C", str(source), *args],
        check=True, capture_output=True, text=True,
    ).stdout

assert git("rev-parse", "HEAD").strip() == lock["commit"]
git("diff", "--exit-code", lock["commit"], "--", "packages")
files = git("ls-tree", "-r", "--name-only", lock["commit"], "--", "packages").splitlines()
extensions = {".ts", ".js", ".mjs", ".cjs", ".c", ".sh", ".sql", ".css", ".html"}
other = {".md", ".json", ".jsonl", ".service", "", ".png", ".node", ".wasm"}
expected = {}
excluded = collections.Counter()
for name in files:
    path = Path(name)
    if (set(path.parts) & {"test", "tests", "__tests__"}
            or re.search(r"\.(test|spec)\.", path.name)
            or name == "packages/coding-agent/examples/extensions/custom-provider-gitlab-duo/test.ts"):
        excluded["test"] += 1
        continue
    if path.suffix in extensions or ("/src/" in name and path.suffix == ".json"):
        data = (source / name).read_bytes()
        expected[name.removeprefix("packages/")] = data.count(b"\n") + int(bool(data) and not data.endswith(b"\n"))
    else:
        assert path.suffix in other, f"Unclassified source kind: {name}"
        excluded[path.suffix or "extensionless"] += 1

name_lock = json.loads((root / "docs/porting/source-lock.json").read_text())
for name in name_lock["additionalSourceFiles"]:
    assert name not in expected, f"Additional source already in original pin: {name}"
    data = subprocess.run(
        ["git", "-C", name_lock["repository"], "show", f"{name_lock['commit']}:packages/{name}"],
        check=True, capture_output=True,
    ).stdout
    expected[name] = data.count(b"\n") + int(bool(data) and not data.endswith(b"\n"))

text = (root / "docs/porting/coverage.md").read_text()
rows = re.findall(r"^\| `([^`]+)` \| (\d+) \|", text, re.M)
assert len(rows) == len(dict(rows)), "Duplicate matrix rows"
actual = {name: int(lines) for name, lines in rows}
assert actual.keys() == expected.keys(), {
    "missing": sorted(expected.keys() - actual.keys()),
    "extra": sorted(actual.keys() - expected.keys()),
}
assert actual == expected, [(name, actual[name], lines) for name, lines in expected.items() if actual[name] != lines]
sections = re.split(r"^## ([\w-]+) \((\d+) files, (\d+) lines\)\n", text, flags=re.M)
assert (len(sections) - 1) // 4 == len({name.split('/')[0] for name in expected})
for index in range(1, len(sections), 4):
    package, count, lines, body = sections[index:index + 4]
    group = re.findall(r"^\| `([^`]+)` \| (\d+) \|", body, re.M)
    assert all(name.startswith(package + "/") for name, _ in group)
    assert len(group) == int(count)
    assert sum(int(n) for _, n in group) == int(lines)
count, lines = re.search(r"\*\*([\d,]+) unique files, ([\d,]+) lines\*\*", text).groups()
assert len(actual) == int(count.replace(",", ""))
assert sum(actual.values()) == int(lines.replace(",", ""))
print(f"PASS: {len(actual)} unique rows, {sum(actual.values())} lines; zero missing/extra/duplicate/count mismatches")
print("Excluded non-source/test categories:", dict(sorted(excluded.items())))
```
