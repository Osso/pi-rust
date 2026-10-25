# Frozen shared control DB and store

Shared state follows `session-control-db.ts` and `multi-agent-store.ts` at [the common frozen pin](../porting/source-lock.json), except the approved zero-delivery-limit and empty-host-retry [deltas](../porting/delta-lock.json). Proof mapping: [control DB traceability](../porting/traceability/session-control-db.md).

## Integration status — 2026-10-21

Canonical checkpoint merge `35d6429097833d094ca448068dae07ce30c482af` integrates `c7248cac`, not completed control/store parity. Subsequent canonical merge `128df8af` integrates the bounded model-tool work through `58301d0a`. Checked items below describe bounded checkpoint fixtures, not whole-module or runtime acceptance. Frozen source remains `4f9e6f8a4`; full repin is ongoing. Coverage and pending waiver revalidation are unchanged.

| Boundary | Checkpoint status / remaining limitation |
|---|---|
| Control/store follow-ups | This integration combines control checkpoint `2284dc6d` and newer bounded branch proofs through `056d2861` with canonical `2f620213`; earlier integration used canonical `4cb53c3143c0da5850242aa47629a001cfe69737`. Follow-up integration is not whole-module acceptance. Bounded proof: `/tmp/pi-rust-repin-control-store/{REPORT.md,ledger.md}` and linked traceability. |
| Canonical raw JSON | The former source→public regression `489d4381` passes unchanged at `65e23c966`: actual source enqueue/read and native public projection/claim preserve U+D800. Earlier status/recovery/routing/steering/migration proofs remain bounded; no whole lossless-authority or store-serialization acceptance is claimed. [Current proof](../porting/traceability/session-control-db.md#canonical-json-counterexample--bounded-regression-now-green). |
| Receipt scanner/equality | Lossless receipt scanning and raw JS equality `06620ae7` are included in integrated control checkpoint `2284dc6d`. Scanner proof does not resolve public payload egress or all canonical writers. |
| Runtime delivery | Prompt/steer delivery now persists recipient receipts and fsyncs before snapshot-fenced deletion; cloned-message tracking prevents duplicate MessageEnd append. Bounded runtime proof: [acceptance trace](../porting/traceability/runtime-mailbox-acceptance.md). Current [acceptance](../porting/traceability/runtime-mailbox-acceptance.md) and [delivery](../porting/traceability/runtime-mailbox-delivery.md) module audits include tool-result delivery, real source/native marker/user/toolResult persistence and normal prompt/steering/interceptor/finally paths. Whole control/store acceptance remains separate; no injected crash/fsync-fault acceptance is claimed. |
| Model publication | Bounded model-tool work through `58301d0a` is integrated by `128df8af`; [bounded model-tool proof](../porting/traceability/agent-model-tool.md) remain distinct from control/store acceptance. Shared-validator repairs through `f86144ba` are integrated by `8d9bb557`. Independent verifier41 (`/tmp/pi-rust-repin-validation-model-verify`) reports model9/9, viewed5/5, validation26 and50-case PASS; historical admission RED is not current. Six library lint diagnostics were fixed at `2284dc6d`; the remaining model-fixture borrow was fixed by `b458d13f`, where the exact model/viewed-target clippy command passed. [Bounded proof](/tmp/pi-rust-repin-control-checkpoint-verify/REPORT.md) excludes later session integration; no whole-control gate is implied. Owned DB model/activity fixtures do not establish whole agent-session or end-to-end model parity. |

Control DB directory creation follows Node dirname/mkdir semantics and the shared node_fs error authority. Preserve native error message, constructor, code, errno, syscall and path through database/session wrappers; selection callers receive the original error rather than an OS Display projection.

## What it must do

- [ ] Preserve canonical persisted JSON UTF16 strings/keys through all reads and mutations. The former source-created lone-surrogate public-body counterexample now passes, as do bounded claim/release/fail/recovery/steering/routing and mailbox migration proofs; group-wide raw public/store authority and egress acceptance remain open; [bounded proof](../porting/traceability/session-control-db.md).

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
- [x] Scan raw receipt refs across all complete transcript records; propagate malformed complete-record errors and ignore partial final records. Preserve lossless SyntaxError and original Node read error constructor, message and filesystem fields.
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
- [x] Honor limit zero without claims/deliveries; retry empty recipientHost with the same enqueue ID and no extra rows. Assert exact approved frozen discrepancies and match the locked delta; retain different-host rejection.
- [x] Preserve frozen mailbox JSON order for public copyMessage outputs and each canonical constructor (direct message, outbound session message, lifecycle notification); byte-level source/native tests assert both boundaries.
- [x] Catch and log original lifecycle, transition and update listener failures locally, continue later listeners and finish publication.
- [x] Preserve original clock/abort callback rejection values and callback sampling order; remove abort handles before invocation; skip empty persistence bindings; preserve locale slot order and first-insertion conflict selection.
- [x] Reject relative nested snapshot/reference paths in frozen copy order before publication; canonicalize copied file references and preserve frozen empty-parent rejection.
- [x] Preserve snapshot root and spread-copied child JSON key order with order-only metadata; emit fixed-copy nested objects in frozen constructor order.
- [x] Preserve original connection/transaction callback throws and rollback state through the existing shared SQLite error source.
- [x] Match legacy prompt-history `slice(0, limit)` return semantics independently of SQLite LIMIT, including negative/zero limits and whitespace-only legacy entries.
- [x] Sort archive candidates by frozen date/UTF-16 path comparison, exclude cutoff-equal/archived/subagent rows and keep selection read-only.
- [x] Preserve raw control-DB agent object/reference-array key order separately from normalized store-copy constructor order, without duplicate JSON values.
- [x] Reject expired detached cancellation commands inside the lifecycle transaction, rolling back agent state and command insertion together.
- [x] Treat empty attachment parents as main-thread roots; reject empty failed/runtime child parents before database access.
- [x] Reject terminal attached-runtime acquisition without mutation and require both incarnation nonces before replacing an alive owner.
- [x] Clear lastActivity on an absent value; preserve currentActivity-before-model insertion order in combined metadata returns and canonical writes.
- [x] Recover superseded live-PID incarnations and replay cleared-owner lost_runtime terminal state without duplicate outbox publication.
- [x] Preserve distinct new-output/existing-reference JSON order in detached finalization and accept reciprocal TS/native replay for completed/failed/aborted outcomes.
- [x] Preserve eligibility predicate rejection/call order and leave every mailbox message unclaimed if candidate collection fails.
- [x] Preserve runtime-registration process-probe rejection before listener/health replacement.
- [x] Wake a verified local mailbox listener for an explicitly empty recipient host through both stored-message and existing-row enqueue APIs.
- [x] Cache runtime process-instance JSON with frozen pid/startTimeTicks/incarnation order and a crypto.randomUUID-compatible UUIDv4 incarnation.
- [x] Permit nested retained-database callbacks on the same connection, including rollback of nested public writes in an outer immediate transaction.
- [x] Skip retention writes when no mailbox/channel rows are due, so ordinary WAL readers succeed while another connection holds the writer lock.
- [x] Normalize public mailbox file references to frozen path/label fields and order without rewriting their original canonical payloads.
- [ ] Resolve canonical lossless JSON and run independent group acceptance.
- [x] Complete the whole multi-agent-store module export/private-branch audit; preserve the separate shared control-DB acceptance boundary.
- [ ] Complete group-wide schema migration/reverse-init, control-DB export/private-branch audit and acceptance; bounded migration/reciprocal proofs do not establish whole control-DB parity.

## How it works

- [Control DB traceability](../porting/traceability/session-control-db.md)
- [Store traceability](../porting/traceability/multi-agent-store.md)

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/session_control_db/` — shared schema and database operations.
- `crates/pi-coding-agent-storage/src/core/multi_agent_store.rs` — persistent/in-memory projection.
- `crates/pi-coding-agent-storage/src/core/session_manager.rs` — metadata snapshot producer.
- `oracle/control-db-oracle.ts` — real frozen TS public-API fixture operations.

## Tests asserting this spec

`crates/pi-coding-agent/tests/repin_control_{schema,sessions,channel,requests,mailbox}.rs`, `repin_runtime_mailbox_transcript.rs`, `approved_mailbox_deltas.rs`.

## Known gaps (current cycle)

- [ ] Finish group-wide source exports/branches and bidirectional cross-state acceptance. Current bounded proof is not whole-module acceptance.
- [x] Add failure-injected atomic relocation proof and TS-primary migration fixtures.
- [x] Reject nested legacy artifact insert/update in both initializers without changing the original row.


## Out of scope

Normal marker/user/tool-result append/fsync/claim acceptance and prompt/steer/interceptor/finally modules now have source/native proof, not whole-group acceptance. Exceptional OS/fsync faults and runtime races follow the current parity exclusions; no injected crash-recovery acceptance is claimed. Relay storage/transport belongs to a separate group. Historical waivers remain pending user revalidation.
