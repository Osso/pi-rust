# Goal / Pyrun re-pin closure

Frozen reference: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Worktree: `/home/osso/.worktrees/pi-rust-repin-bundled-ext`.
Last production edit: `472539a8e`; row/fixture closure: `72a2b695c`.
Parent API integration: `de46a2789`.

All fourteen assigned rows are `done`: seven goal modules and seven Pyrun modules.
Source counts remain locked at 686 rows / 186,679 lines. Repository-wide status at
this checkpoint: done 615, gap 25, partial 25, todo 21; other lanes own those open rows.

## Proof ledger

All commands run from the explicit worktree. Tests use the mandatory runtime guard,
private artifacts and fake/local runners; no host providers or user state are used.

| Scope / command | Result / log |
| --- | --- |
| `cargo check -j3 --workspace --all-targets` after the final production edit | PASS; `/tmp/bundled3-workspace-check-last.log` |
| `cargo fmt --all --check` before every logical commit | PASS |
| `cargo test -j3 -p pi-coding-agent --test suite_extensions goal_` after the shared context/session-manager merge | 36 PASS; `/tmp/bundled3-goal-after-context-merge.log` |
| core-1/core-2/extension Pyrun filters before the final runner environment edit | 23 + 5 + 20 PASS; `/tmp/bundled3-pyrun-integration2.log`; pure artifact/manifest/replay proof scope remains unchanged |
| final extension runner tests after the environment edit | 17 PASS; `/tmp/bundled3-pyrun-extension-final.log` |
| final detached runner process / real canonical Pyrun / registration targets | 8 + 2 + 9 PASS; `/tmp/bundled3-pyrun-process-final.log` |
| harness-free fresh foreground and detached initial launch | PASS; same final process log, two real launches, exact one terminal record, revision and process-exit assertions |
| fourteen strict frozen-source oracle fixtures | PASS; `/tmp/bundled3-types-complete.log`; `oracle/bundled-ext-goal-pyrun-typecheck.mjs` uses the approved scoped ES2024 noEmit target |
| focused extension library and changed fixture clippy, warning mode | PASS; `/tmp/bundled3-clippy-final.log`, `/tmp/bundled3-fixture-clippy-final.log`; existing warnings remain, zero locations intersect changed lane lines (`/tmp/bundled3-clippy-changed-lines-final.log`) |
| `python3 scripts/source-inventory.py` | PASS; `/tmp/bundled3-inventory-final.log` |

No test/check is rerun only for a commit or milestone. Later reruns above follow a
relevant source edit or the shared context/session-manager merge. The final runner
option omission/environment tests add actual frozen differential proof to their
respective source edits; earlier fully specified manifest fixture values are unchanged.

## Audit and exclusions

Whole-module export/branch maps live in the goal traceability documents and
[pyrun-extension](traceability/pyrun-extension.md). Rust and TypeScript changes were
manually audited for clarity, nesting, state, error propagation and lifecycle cleanup;
metric CLIs are unavailable. Dense TypeScript fixtures were formatted with the pinned
Biome executable. No lint suppression, ignored test, new dependency or fallback was added.

The missing `ctx.switchSession` guard in Pyrun is classified as frozen-unreachable:
the frozen runner always installs the wrapper and all real hosts bind it. Main
confirmed that no context API change or gap is required. The abandoned synthetic
absence test was removed; no core context API files were changed for it.

Standing policy gaps are explicit in each module trace: pathological/wrong-typed
inputs beyond existing carriers, unusual OS/runtime failure shapes (including activation
write-failure injection), Node scheduling oddities, Windows-only process behavior,
Node stack text and native environment enumeration order. They are not widened into
new APIs or claimed as newly approved semantic deviations.

The initial process integration exposed fixture bugs, not accepted failures:
ownership named the parent instead of the actual runner child, and the resume
fixture could complete before observation. Both were corrected without loosening
assertions. The hung test groups were cancelled and their exit verified; the corrected
real-process tests pass. No full workspace test suite, push, release or deployment
was performed; main owns the merge-train workspace gate and independent verification.
