# Persisted session tool output

The frozen `core/session-tool-output.ts` contract limits persisted tool results without changing runtime messages. [Source/branch proof](../porting/traceability/session-tool-output.md).

## What it must do

- [x] Match frozen text-byte, serialized-content and complete-entry caps, including details removal and minimal identity truncation; preserve unknown fields and exact UTF-16 before the source UTF-8 boundary.
- [x] Preserve frozen raw-entry/property/Buffer.from errors; never silently omit invalid primitive identities.
- [x] Scan recursive JSONL files in frozen report order; classify malformed, non-session and per-file failures identically.
- [x] Preserve original bytes in a backup, publish capped content atomically with the same mode, and skip changed files including negative-epoch timestamp races.
- [x] Catch clock callback throws per file, preserving Error.message or String(value); invalid dates report the frozen RangeError message. Callback and catch-conversion failures are fallible.

## How it works

- [Module traceability and proof ledger](../porting/traceability/session-tool-output.md).

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/session_tool_output.rs`: truncation, serialization and migration.
- `crates/pi-coding-agent-storage/src/core/session_manager.rs`: persisted append/rewrite/fork callers.
- `crates/pi-coding-agent-tools/src/core/external_session_importer.rs`: imported session serialization.
- `crates/pi-coding-agent/src/cli/sessions_command.rs`: CLI error propagation.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/repin_storage_tool_output.rs`
- `crates/pi-coding-agent/tests/session_tool_output.rs`
- `crates/pi-coding-agent/tests/differential_session_manager.rs`
- `crates/pi-coding-agent/tests/lossless_session_persistence.rs`
- `crates/pi-coding-agent/tests/repin_session_storage.rs`

## Known gaps (current cycle)

- [ ] Approved2026-10-25: non-string Buffer-like array/array-like/object identities at the minimal fallback are rejected explicitly; frozen may preserve the original value. No single native String conversion exists.

## Out of scope

JavaScript object identity (owned-value approval), Windows-only runtime behavior (approved platform gap), and live-state migration during port verification.
