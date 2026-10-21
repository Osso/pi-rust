# Metadata-authoritative session names

Session display names follow the authorized [common source pin](../porting/source-lock.json) (former c248b0067 approval to re-validate against 4f9e6f8a4), replacing the pinned v14 duplicate store. Behavioral mappings live in the [control DB](../porting/traceability/session-control-db.md), [manager](../porting/traceability/session-manager.md), and [autoname](../porting/traceability/session-autoname.md) traceability docs.

## What it must do

- [x] Migrate v14 names under lifecycle quiescence; normalize line breaks, prefer matching legacy names over metadata, discard orphan names, drop the duplicate table, and finish at schema15 without rerunning lifecycle14 on existing v15 DBs.
- [x] Make metadata authoritative: NULL means never named, empty means explicitly cleared, and generic metadata updates preserve both dedicated names and explicit clears.
- [x] Ignore historical JSONL names; reset/rehydrate names on switching and do not inherit names into forks or copied imports.
- [x] Persist CLI/RPC name changes without appending JSONL session-info entries.
- [ ] Expose frozen `set_session_name` model tool with `{name}` and the exact prompt guideline; never issue a separate title-provider request.
- [ ] Collapse all line breaks and JavaScript whitespace, trim, then truncate at the frozen 80-UTF16-unit word boundary; preserve exact tool results and SQLite storage.
- [ ] Refuse blank, child, ephemeral, pre-named and explicitly-cleared sessions; allow repeated updates only while the current name equals this factory's last successful tool name.
- [ ] Preserve closure ownership through session-start/switch events; reset only when the factory is registered again. The frozen module has no session-start listener.

## How it works

- [Control DB traceability](../porting/traceability/session-control-db.md)
- [Manager traceability](../porting/traceability/session-manager.md)
- [Autoname traceability and pending representation boundary](../porting/traceability/session-autoname.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/session_control_db/` — schema migration and authoritative name storage.
- `crates/pi-coding-agent/src/core/session_manager.rs` — cached name state and lifecycle resets.
- `crates/pi-coding-agent/src/core/agent_session{,_runtime}.rs` — name events and imported-session metadata removal.
- `crates/pi-coding-agent/src/{main.rs,cli/session_picker.rs,modes/interactive/interactive_mode.rs}` — startup/selector name paths.
- `crates/pi-coding-agent/src/bundled_extensions/session_autoname.rs`, `crates/pi-js-compat/src/string.rs` — model naming tool and string semantics.

## Tests asserting this spec

`crates/pi-coding-agent/tests/{session_control_db,session_manager,agent_session,agent_session_runtime,session_autoname,startup_session_name,rpc_mode_session_name,cross_implementation_control_db,differential_control_db}.rs`; `crates/pi-js-compat/tests/string_utf16.rs`.

## Known gaps (current cycle)

Frozen naming-tool ordinary differential, persistence and mutation proofs are in progress. Split-surrogate tool results cannot be represented by the current UTF8 extension setter/result fields; whole-module parity remains pending. Exact fixtures and proof status belong in [autoname traceability](../porting/traceability/session-autoname.md).

## Out of scope

DB/manager migration and shared extension API changes belong to their respective owners. The October 20, 2026 intermediate-string waiver is historical and pending revalidation against the new source; this tool does not silently renew it. General source pin and telemetry behavior remain unchanged.
