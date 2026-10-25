# Session cwd

Preserve the frozen `core/session-cwd.ts` contract at the [common pin](../porting/source-lock.json). [Branch matrix and proof](../porting/traceability/session-cwd.md).

## What it must do

### Lookup and errors

- [x] Find the nearest existing directory, following directory symlinks but rejecting regular files as candidates; ignore only ENOENT/ENOTDIR during ascent.
- [x] Propagate other stat failures with exact Node error class, UTF-16 message and native properties.
- [x] Skip recovery for absent/empty session-file names and empty/existing stored cwd values before examining the fallback.
- [x] Preserve `MissingSessionCwdError` name, UTF-16 message, issue fields and original native error type/source through CallbackError.
- [x] Format errors/prompts with frozen empty-string truthiness and exact UTF-16.

### Reopen and resume

- [x] Main `--session` opens another project's original session without prompting or forking; archived selection restores through the control DB before open.
- [x] Bare compressed transcript names count as paths even when absent. Native main regressions assert both boundaries.

- [x] Reject absent/empty session-file names before reopen I/O; preserve selected cwd, optional session directory and the persisted control-db cwd.
- [x] Resume prompts use the caught MissingSessionCwdError issue, even if the session file changes before the catch; never reopen the file to reconstruct the error.

## How it works

- [Session cwd traceability](../porting/traceability/session-cwd.md)
- [SessionManager option-truthiness audit](../porting/traceability/session-manager.md#common-pin-truthiness-audit-2026-10-23)

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/session_cwd.rs` — lookup, native errors, formatting and reopen.
- `crates/pi-coding-agent-storage/src/core/session_manager.rs` — delegated constructor/listing truthiness and cwd authority.
- `crates/pi-coding-agent/src/{main.rs,core/agent_session_runtime.rs,modes/interactive/interactive_mode.rs}` — fallible callers and resume dispatch.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/session_cwd.rs` and `oracle/session-cwd-oracle.mts` — actual frozen module, real filesystem and metadata boundaries.
- `crates/pi-coding-agent/tests/session_manager_truthiness.rs` and `oracle/session-manager-truthiness-oracle.mts` — constructor/listing regressions.
- `ui_tests::resume_missing_cwd_uses_caught_issue_after_session_file_changes_like_frozen` and `oracle/interactive-resume-cwd-error-oracle.mts` — real runtime error delivery, intervening file rewrite, confirmation and cancellation status.

## Known gaps (current cycle)

- [ ] The fixed-point “No existing directory found” guard has source-correspondence proof only: a normal POSIX root/current-directory inode is a directory, so the branch cannot be reproduced with the real filesystem fixtures. No fake filesystem seam was added.

## Out of scope

Before-run/after-turn recovery lifecycle hooks remain in the [runtime re-port](agent-session-runtime.md). Invalid JavaScript runtime input types and approved platform gaps follow [project rules](../../AGENTS.md).
