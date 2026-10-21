# Frozen archive/delete/history re-port proof

Frozen: [4f9e6f8a4 lock](source-lock.json), read-only. Worktree `/home/osso/.worktrees/pi-rust-repin-session-archive-history`, branch `repin/session-archive-history`. Test revisions `3d4e6a32` (four focused targets) and `d6b4bed5` (expanded command target); production last changed `baac3d66`. Raw evidence and command ledger: `/tmp/pi-rust-repin-session-archive-history/REPORT.md` and sibling logs.

| Coverage | Result | Evidence |
|---|---|---|
| bundled command registration, archive/delete/unarchive branches, teardown after final writes | 10 PASS (one worker entry point) | `command-errors-green-2.log`, mandatory Cargo sandbox |
| session tools including frozen conversational history registration + 25 queries | 10 PASS / 1 excluded | same; unrelated `ask_questions_tool_differential` remains known failing and untouched |
| native history behavior | 8 PASS | same |
| delete process/tree/batch/partial cleanup/errors | 4 PASS (one worker entry point) | same; temporary SQLite/files/fake trash only |
| production mutants | 3 killed | `mutant-history-self-call`, `mutant-archive-quit-gate`, `mutant-delete-metadata` logs/patches; restored before final GREEN |

Every Cargo invocation used configured sandbox runner, `CARGO_BUILD_JOBS=4`, and worktree-local `CARGO_TARGET_DIR`. No live sessions/state touched. Formatting passed before each commit. No push/merge/delegation/clippy or broad suites. This is bounded implementation evidence, not milestone acceptance or independent verification.

Readability: manual changed-line Rust/TS audit; metrics executable unavailable. Split quit orchestration and stderr collection; removed filesystem-side mutation inside filter. Inherited long history execute/searchable-entry functions remain; no unrelated readability redesign attempted. Shutdown errors now have exact subprocess stderr differential proof. Pathological non-Linux/UTF-16 process cases remain proof limits, not approved semantic deviations.

Detailed mapping: [archive](traceability/session-archive.md), [delete](traceability/session-delete.md), [history](traceability/search-current-session-history.md).

## Verifier corrections (2026-10-21)

Supersedes the numeric/executable-text/split-surrogate proof limits above; does not waive unrelated failures. Implementation/test revision `9bb38edf` (production `b56ca583`). Evidence: `/tmp/pi-rust-repin-session-archive-history-fixes/`; appended ledger in `/tmp/pi-rust-repin-session-archive-history/REPORT.md`.

| Exact capability | Proof |
|---|---|
| JS binary64 numeric argument coercion: `1.0` excluded, `0.000001` included, `1e-6` excluded | RED frozen differential; 30 reopened-session queries and three live-manager queries GREEN; serde-format mutant killed on live `1.0` |
| Linux Node executable text without shebang | RED original `exit 7` probe through frozen `spawn`; GREEN exact diagnostic and recorded `$0`/`--`/literal metacharacter-path argv; ENOEXEC mutant killed |
| UTF-16 stderr slice, trim/first-line and public deletion error | RED U+FFFD vs U+D83D; GREEN exact public code units and JSON serialization, including `\\ud83d`; `JsString` retained until terminal UTF-8 output boundary |
| Four requested targets, sandboxed, no exclusions | `final-four-targets.log`: bundled commands 10 PASS; session-tools 11 PASS / 1 FAIL; native history 8 PASS; deletion 6 PASS (worker included) |
| Still failing / outside lane | `ask_questions_tool_differential`: omitted explicit blocking remains accepted by Rust and rejected by frozen; untouched |

Final target command exits 101 solely for that known ask_questions failure: 35 PASS / 1 FAIL. All Cargo test calls retain configured mandatory sandbox, four build jobs and worktree-local target. Mutations restored before final run; subsequent changes documentation-only. No new clippy/check, independent verification, cross-platform diagnostics or malformed UTF-8-stream claims. Source inventory passes unchanged 686 rows. Manual changed-line Rust/TS readability audit; metrics binary unavailable. Frozen checkout remains clean at the common pin; no push, merge or delegation.
