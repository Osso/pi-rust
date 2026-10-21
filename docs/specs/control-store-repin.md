# Frozen shared control DB and store

Shared state follows `session-control-db.ts` and `multi-agent-store.ts` at [the common frozen pin](../porting/source-lock.json). Proof mapping: [control DB traceability](../porting/traceability/session-control-db.md).

## What it must do

- [x] Read and update TS-created metadata, and survive TS initialization of Rust-created metadata, without obsolete transcript search columns.
- [x] Preserve authoritative NULL versus explicit cleared names during metadata updates.
- [x] Select archive candidates without archiving; expose atomic relocation plus archive/unarchive operations.
- [x] Resolve direct/transitive subagent paths through plain and `.zst` aliases; remove metadata in bulk.
- [x] Assign missing mailbox birth on INSERT; enforce unique imported channel origins and restriction request IDs.
- [ ] Match retention, canonical mailbox CAS/recovery, channel projection/cursors, supervisor cancellation, agent terminal replay/suppression/model updates and whole in-memory store contracts.

## How it works

- [Control DB traceability](../porting/traceability/session-control-db.md)
- [Store traceability](../porting/traceability/multi-agent-store.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/session_control_db/` — shared schema and database operations.
- `crates/pi-coding-agent/src/core/multi_agent_store.rs` — persistent/in-memory projection.
- `crates/pi-coding-agent/src/core/session_manager.rs` — metadata snapshot producer.
- `oracle/control-db-oracle.ts` — real frozen TS public-API fixture operations.

## Tests asserting this spec

`crates/pi-coding-agent/tests/repin_control_{schema,sessions}.rs`.

## Known gaps (current cycle)

- [ ] Finish group-wide source exports/branches and bidirectional cross-state proof. Current foundation proof is not whole-module acceptance.
- [ ] Add failure-injected atomic relocation proof and migration fixtures.

## Out of scope

Runtime message append/fsync acceptance belongs to later delivery integration. Relay storage/transport belongs to a separate group. SQLite fixtures do not prove crash-safe recipient acceptance. Historical waivers remain pending user revalidation.
