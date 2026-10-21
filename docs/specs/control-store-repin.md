# Frozen shared control DB and store

Shared state follows `session-control-db.ts` and `multi-agent-store.ts` at [the common frozen pin](../porting/source-lock.json). Proof mapping: [control DB traceability](../porting/traceability/session-control-db.md).

## Integration status — 2026-10-21

Canonical checkpoint merge `35d6429097833d094ca448068dae07ce30c482af` integrates `c7248cac`, not completed control/store parity. Subsequent canonical merge `128df8af` integrates the bounded model-tool work through `58301d0a`. Checked items below describe bounded checkpoint fixtures, not whole-module or runtime acceptance. Frozen source remains `4f9e6f8a4`; full repin is ongoing. Coverage and pending waiver revalidation are unchanged.

| Boundary | Checkpoint status / remaining limitation |
|---|---|
| Control/store follow-ups | This integration combines control checkpoint `2284dc6d` with canonical `4cb53c3143c0da5850242aa47629a001cfe69737`; follow-up integration is not whole-module acceptance. Bounded proof: `/tmp/pi-rust-repin-control-store/{REPORT.md,ledger.md}` and linked traceability. |
| Canonical raw JSON | Source→public regression `489d4381` remains OPEN: source-created escaped U+D800 projects as U+FFFD because public body remains `String`. Status claim/release/fail `eef3707b` and recovery `429d6033` preserve original raw JSON with bounded source-peer proof; source SQLite keys and receipt identities remain distinct. Routing/steering/store/migration raw writers and scalar widening remain pending; no whole lossless-authority claim or waiver. |
| Receipt scanner/equality | Lossless receipt scanning and raw JS equality `06620ae7` are included in integrated control checkpoint `2284dc6d`. Scanner proof does not resolve public payload egress or all canonical writers. |
| Runtime delivery | Acceptance is currently eager, before durable fsync. Claim → receipt append → persist/fsync → fenced delete integration remains pending in the delivery lane. SQLite fixtures do not prove this boundary. |
| Model publication | Bounded model-tool work through `58301d0a` is integrated by `128df8af`; [bounded model-tool proof](../porting/traceability/agent-model-tool.md) remain distinct from control/store acceptance. Shared-validator repairs through `f86144ba` are integrated by `8d9bb557`. Independent verifier41 (`/tmp/pi-rust-repin-validation-model-verify`) reports model9/9, viewed5/5, validation26 and50-case PASS; historical admission RED is not current. Six lint diagnostics were fixed at `2284dc6d` by boxing the enum payload and removing five borrows; invalidated exact clippy has not been rerun. Owned DB model/activity fixtures do not establish whole agent-session or end-to-end model parity. |

## What it must do

- [ ] Preserve canonical persisted JSON UTF16 strings/keys through all reads and mutations. Public projection remains lossy; bounded status/recovery preservation does not complete routing/steering/store/migration writers or egress; [blocking proof](../porting/traceability/session-control-db.md).

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
- [x] Consume direct steering or atomically claim runtime steering with its lifecycle advance; roll back both writes on message CAS failure.
- [x] Honor suppression in detached finalization/dead recovery and preserve finalization replay after outbox deletion; persist same-lifecycle cancellation suppression without duplicate commands.
- [x] Filter delivered legacy outbox rows, migrate missing parent/obsolete search columns, merge legacy counters, strip artifact fields and enforce live-owner migration quiescence.
- [x] Match SQLite cleanup at controlled exactly24h/24h−1ms and expire missing/null/non-text/invalid births.
- [x] Preserve legacy live process identities without incarnation; supersession requires both nonces.
- [x] Preserve escaped lone-surrogate receipt values/keys and malformed ref elements losslessly; compare raw refs with JS property/strict-equality semantics.
- [x] Exercise fresh Rust→TS initialization→Rust mutation→same TS reads across mailbox, agent/outbox, channel, supervisor and base control state.
- [x] Fence delivery against a real peer payload update; abort on malformed later complete receipts before deleting, and honor raw ref short-circuit errors.
- [x] Prove expected-payload CAS with an explicit guard-disabled negative control and byte-identical restoration.
- [ ] Classify source limit0 and empty-host retry branches; resolve canonical lossless JSON and run independent group acceptance.
- [ ] Complete group-wide schema migration/reverse-init, export and whole-store acceptance; bounded migration, reciprocal and reset/error proofs do not establish whole-module parity.

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

- [ ] Finish group-wide source exports/branches and bidirectional cross-state acceptance. Current bounded proof is not whole-module acceptance.
- [x] Add failure-injected atomic relocation proof and TS-primary migration fixtures.
- [x] Reject nested legacy artifact insert/update in both initializers without changing the original row.


## Out of scope

Runtime message append/fsync acceptance belongs to later delivery integration and remains a completion blocker, not an approved gap. Older runtime mark/consume callers still require durable acceptance before source-style deletion. Relay storage/transport belongs to a separate group. SQLite fixtures do not prove crash-safe recipient acceptance. Historical waivers remain pending user revalidation.
