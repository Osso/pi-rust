# Interactive session import

Match frozen `interactive-mode.ts:7237–7313` path parsing and import control flow. [Source/proof map](../porting/traceability/interactive-mode.md#path-arguments-and-import-command-2026-10-25).

## What it must do

- [x] Parse `/import` and `/export` arguments using the exact command-space prefix, JavaScript whitespace, first quoted delimiter and UTF-16 slices. Preserve an explicitly quoted empty argument separately from an absent argument.
- [x] Confirm before importing; cancellation preserves the current session/status and emits `Import cancelled`. After confirmation, stop the loading animation and clear the status container.
- [x] Honor a runtime before-switch cancellation. Successful import emits `Session imported from: <path>` and restores imported messages.
- [x] A typed missing cwd prompts for fallback; rejection emits cancellation. A failed retry rejects at the original await boundary rather than becoming a recoverable error or a new fatal catch.
- [x] Typed missing-file errors remain recoverable; an ordinary factory Error with similar text is fatal, with the exact full frame and exit1.
- [ ] Prove successful fallback-cwd retry and a cancellation returned by the retry.
- [ ] Preserve the source in-memory import empty-directory failure before runtime callbacks; it is not a new native recovery path.

## How it works

- [Interactive map](../porting/traceability/interactive-mode.md#path-arguments-and-import-command-2026-10-25)
- [Runtime/error authority](../porting/traceability/agent-session-runtime.md#missing-import-error-identity-2026-10-25)

## Implementation inventory

- `crates/pi-coding-agent-interactive/src/modes/interactive/interactive_mode.rs` — argument helper, confirmation, import outcomes and typed recovery.
- `crates/pi-coding-agent-core/src/core/agent_session_runtime.rs` — persistence and lifecycle authority.

## Tests asserting this spec

- `tests/support/interactive_path_commands.rs` / `oracle/interactive-path-command-oracle.mts` — 26 actual-source argument cases, including quoted UTF-16.
- `tests/support/interactive_startup.rs` / `oracle/interactive-startup-oracle.mts` — actual mode/runtime processes, dialog/chat/status frames, imported messages/file-copy state, retry rejection and exit codes.

## Known gaps (current cycle)

The two unchecked branches above remain in this active slice. Whole interactive-mode acceptance is separate.

## Out of scope

Other export/share command algorithms and any source bug fix or new recovery behavior are excluded.
