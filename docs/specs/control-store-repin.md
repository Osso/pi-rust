# Frozen shared control DB and store

Shared state follows `session-control-db.ts` and `multi-agent-store.ts` at [the common frozen pin](../porting/source-lock.json). Proof mapping: [control DB traceability](../porting/traceability/session-control-db.md).

## What it must do

- [x] Read and update TS-created metadata, and survive TS initialization of Rust-created metadata, without obsolete transcript search columns.
- [x] Preserve authoritative NULL versus explicit cleared names during metadata updates.
- [x] Select archive candidates without archiving; expose atomic relocation plus archive/unarchive operations.
- [x] Resolve direct/transitive subagent paths through plain and `.zst` aliases; remove metadata in bulk.
- [x] Assign missing mailbox birth on INSERT; enforce unique imported channel origins and restriction request IDs.
- [x] Preserve channel origins, count origin-tagged same-session messages as pending, and advance cursors monotonically across peers.
- [x] Expire old channel messages during DB initialization.
- [x] Preserve cancelled supervisor status; cancel only matching sender's pending/claimed requests, clear claim token and reject late completion.
- [x] Retry missing-birth enqueues idempotently without accepting changed message identity.
- [x] Claim before delivery; reject stale payload deletion; CAS-delete after explicit delivery and never consume foreign-host rows locally.
- [x] Hide expired mailbox rows by creation age; delete dead claims with complete transcript acceptance rather than replaying.
- [x] Scan raw receipt refs across all complete transcript records; propagate malformed complete-record errors and ignore partial final records.
- [x] Preserve original mailbox birth through updates, delete terminal outbox acknowledgement rows, and accept identical terminal replay after peer acknowledgement removed the row.
- [x] Expire in-memory mailbox at exactly24h, evict peer-deleted rows, and consume/remove while returning the original message snapshot.
- [x] Enforce store terminal-notification suppression; preserve activity `detail` and publish model/current activity under exact DB ownership.
- [x] Sweep expired retained-connection messages on the source minute timer; default null birth once and project unknown runtime status to failed.
- [ ] Prove steering delivery, detached finalization replay/suppression and remaining whole-store branches.

## How it works

- [Control DB traceability](../porting/traceability/session-control-db.md)
- [Store traceability](../porting/traceability/multi-agent-store.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/session_control_db/` — shared schema and database operations.
- `crates/pi-coding-agent/src/core/multi_agent_store.rs` — persistent/in-memory projection.
- `crates/pi-coding-agent/src/core/session_manager.rs` — metadata snapshot producer.
- `oracle/control-db-oracle.ts` — real frozen TS public-API fixture operations.

## Tests asserting this spec

`crates/pi-coding-agent/tests/repin_control_{schema,sessions,channel,requests,mailbox}.rs`, `repin_runtime_mailbox_transcript.rs`.

## Known gaps (current cycle)

- [ ] Finish group-wide source exports/branches and bidirectional cross-state proof. Current foundation proof is not whole-module acceptance.
- [ ] Add failure-injected atomic relocation proof and migration fixtures.

## Out of scope

Runtime message append/fsync acceptance belongs to later delivery integration and remains a completion blocker, not an approved gap. Older runtime mark/consume callers still require durable acceptance before source-style deletion. Relay storage/transport belongs to a separate group. SQLite fixtures do not prove crash-safe recipient acceptance. Historical waivers remain pending user revalidation.
