# Source inventory reconciliation

Pinned source: [source-lock.json](source-lock.json). Matrix: [coverage.md](coverage.md).

## Scope and counting

Inventory covers all five directories under `packages/`, including bundled extensions, examples, tooling, native addons and vendored sources. Select tracked `.ts` (including `.d.ts`), `.js`, `.mjs`, `.cjs`, `.c`, `.sh`, `.sql`, `.css`, `.html`, plus `.json` anywhere under `src/`. Do not restrict discovery to top-level package `src/` directories. Public `testing/` conformance helpers are source, not test suites.

Exclude directories named `test`, `tests`, `__tests__`, filenames containing `.test.` or `.spec.`, and the standalone GitLab Duo example `test.ts` (test driver). Runnable demonstrations such as `overlay-test.ts` remain inventoried; test-runner configuration is tooling source. Generated/vendored tracked source is included; ignored generated data is not part of the pinned Git tree.

Line count = LF bytes plus one for a nonempty unterminated final line. Other tracked files are metadata/configuration/data/docs or binary assets, not text-source rows: `.md`, `.json` outside `src/`, `.jsonl`, `.service`, extensionless dotfiles, `.png`, `.node`, `.wasm`. Validator reports these categories and rejects unknown extensions. Binary assets remain outside source totals; parity is not established by this inventory.

## Re-pin reconciliation (verified: 2026-10-21)

User-approved target: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. Previous Rust matrix at `8035c95a`: 820 rows / 191,994 lines, including the separately approved session-autoname addition. The new pin contains that module directly; no additional-source lock or inventory extension remains.

Active matrix: **686 unique files / 186,679 lines** in five packages. Statuses: `done` 299, `gap` 12, `todo` 375, `partial` 0. These are inventory labels, not acceptance evidence. Byte-identical rows retain old status/mapping and carry “re-run vs 4f9e6f8a4 pending”. Modified/renamed rows are `todo`, preserving old status/mapping in notes; added rows are `todo`, “new on master”. Old approvals are pending revalidation. Removed rows live outside active totals; Rust deletion is deferred.

[Recorded classification](repin-classification.json) is the main thread's source-only `git diff --name-status -M50% 532f7d12 4f9e6f8a4` result (272 M, 213 D, 80 A, 23 R); neither old checkout nor old Git objects are available. Regeneration consumes that supplied evidence and the tracked old matrix, never recreates the old source. Its 687 master source paths include the excluded standalone GitLab Duo `test.ts`; the selection rules above yield 686 active rows. The old authorized session-autoname row is classified A relative to the original general pin and becomes `todo`, not silently promoted.

## Repeatable validation

From the Rust checkout:

```text
python3 scripts/source-inventory.py
python3 scripts/source-inventory.py --write
python3 -m unittest discover -s scripts -p test_source_inventory.py
```

[Validator/generator](../../scripts/source-inventory.py) reads the lock, verifies HEAD and tracked package bytes, validates selection completeness, duplicate rows, line counts, package subtotals, header totals, row statuses and displayed status totals. Legacy rows with Rust mappings stored in the Level column are normalized before regeneration; proof text moves to Notes without loss. `--write` regenerates using the frozen old Rust matrix (`8035c95a`) and recorded classification; it does not establish behavior parity. Behavioral fixtures cover source selection, line counts, unchanged/modified/renamed/added/removed policies and rejected matrix mutations. Unknown extensions fail explicitly.
