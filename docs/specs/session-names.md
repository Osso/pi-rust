# Metadata-authoritative session names

Session display names follow the authorized [common source pin](../porting/source-lock.json) (former c248b0067 approval to re-validate against 4f9e6f8a4), replacing the pinned v14 duplicate store. Behavioral mappings live in the [control DB](../porting/traceability/session-control-db.md), [manager](../porting/traceability/session-manager.md), and [autoname](../porting/traceability/session-autoname.md) traceability docs.

## What it must do

- [x] Migrate v14 names under lifecycle quiescence; normalize line breaks, prefer matching legacy names over metadata, discard orphan names, drop the duplicate table, and finish at schema15 without rerunning lifecycle14 on existing v15 DBs.
- [x] Make metadata authoritative: NULL means never named, empty means explicitly cleared, and generic metadata updates preserve both dedicated names and explicit clears.
- [x] Ignore historical JSONL names; reset/rehydrate names on switching and do not inherit names into forks or copied imports.
- [x] Persist CLI/RPC name changes without appending JSONL session-info entries.
- [x] Register the frozen `set_session_name` tool for persisted main sessions, with exact metadata, refusal ordering and returned content/details.
- [x] Collapse whitespace and cap names at 80 UTF-16 units without removing formatting; preserve manual names and explicit clears, allow this tool to revise its own name, and propagate write failures without claiming success.
- [x] Match TypeScript's SQLite stored names when title truncation splits a surrogate pair; use U+FFFD rather than dropping the sliced code unit.

## How it works

- [Control DB traceability](../porting/traceability/session-control-db.md)
- [Manager traceability](../porting/traceability/session-manager.md)
- [Naming tool traceability and historical helper gap](../porting/traceability/session-autoname.md)

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/session_control_db/` — schema migration and authoritative name storage.
- `crates/pi-coding-agent-storage/src/core/session_manager.rs` — cached name state and lifecycle resets.
- `crates/pi-coding-agent-core/src/core/agent_session{,_runtime}.rs` — name events and imported-session metadata removal.
- `crates/pi-coding-agent/src/{main.rs,cli/session_picker.rs,modes/interactive/interactive_mode.rs}` — startup/selector name paths.
- `crates/pi-coding-agent-extensions/src/bundled_extensions/session_autoname.rs`, `crates/pi-js-compat/src/string.rs` — model-invoked naming tool and UTF-16 normalization.

## Tests asserting this spec

`crates/pi-coding-agent/tests/{session_control_db,session_manager,agent_session,agent_session_runtime,session_autoname,startup_session_name,rpc_mode_session_name,cross_implementation_control_db,differential_control_db}.rs`; `crates/pi-js-compat/tests/string_utf16.rs`.

## Known gaps (current cycle)

No outstanding behavior gap in this scope; the approved intermediate-string representation difference is listed below.

## Out of scope

Exact in-memory representation of JS lone-surrogate helper strings: user approved UTF-8 U+FFFD representation on October 20, 2026 (“Match DB; record gap”), but not a storage discrepancy. General source pin and telemetry behavior remain unchanged.
