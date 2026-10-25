# Storage-3 checkpoint — 2026-10-25

Branch `repin/core-tools`; baseline2683e5cbc, parent77c92996b merged by instruction. Frozen [common pin](source-lock.json) remains read-only and unchanged. No child agents, broad suites, live providers, push or deployment. Parent owns final integration acceptance. [Previous checkpoint](storage-successor-proof.md).

## Closed assigned modules

All eleven assigned module rows are closed. Control-DB completion follows the user's lighter-proof directive: compact reciprocal persistence per table/flow, audit/fix/native proof for non-persistence internals; no further large matrices.

| Modules | Current capability / proof |
|---|---|
| runtime-mailbox-transcript | All3 exports/realistic branches; original Node read records and actual SyntaxError constructor, lossless complete-record scanning and raw refs; source/native6 tests |
| session-history + transcript-spool | Whole82/71-line audits; lazy ancestor order, malformed/inactive/missing ancestors, exact Node open and codec errors, archive/large Unicode/early drop. History4 current tests plus4 storage/history integrations; prior spool6 and actual90s proof remain applicable (copy visibility only). [History](traceability/session-history.md), [spool](traceability/session-transcript-spool.md) |
| session-archive-storage + idle-session-archive | Whole99/240-line audits; archive original read/codec/DB errors, reciprocal bytes/modes/rollback/plain precedence and short circuits. New archive4 + reciprocal2 pass. Idle unchanged mapped full-pass/publication/startup proof remains applicable. [Archive](traceability/session-archive-storage.md), [idle](traceability/idle-session-archive.md) |
| terminal-outbox-delivery | Whole84-line audit;6 reciprocal lifecycle publication/ack directions,8 retry/poison/mismatch/peer-claim cases, actual artifact cleanup and2 later-clock/finally cases;5 grouped tests with existing numeric/clock matrix pass. No production correction needed. [Trace](traceability/terminal-outbox-delivery.md) |
| runtime-mailbox-acceptance + delivery + extensions/multi-agent barrel | Whole82/55/21-line audits; reciprocal marker/user/toolResult6 cases, valid/absent descriptors5 cases, role/empty guards, release, actual runtime intercept ordering and exact unhandled prompt. Acceptance3 and runtime11 pass. Native barrel maps all9 values/8 types without adding an API. [Acceptance](traceability/runtime-mailbox-acceptance.md), [delivery](traceability/runtime-mailbox-delivery.md), [barrel](traceability/extensions-multi-agent.md) |
| multi-agent-store | Whole1467-line audit; original callback/clock/abort and live listener behavior, locale slots, exact public/canonical mailbox bytes,33 snapshot copy/path/JSON scenarios, empty-parent rejection, restore/reset/counters/expiry and reciprocal persisted state. Current28 store tests, operation script and bounded error/wire matrices pass. [Map](traceability/multi-agent-store.md) |
| session-control-db | Whole7547-line/127-export and private-branch audit; canonical/public UTF-16 bytes, v15 migrations, lifecycle/ownership/CAS/recovery/finalization, callbacks and retained connection behavior. Compact reciprocal8 flows plus native reentry/reference-wire tests pass at76d25b687 (only a callback type alias after runtime-tested d69e6199c); final routing transaction RED→GREEN expands one native routing byte/order/idempotence test. [Current map](traceability/session-control-db.md) |

## Corrections and gates

Production corrections: receipt scanner Node FS/SyntaxError records; history open/decoder records; archive read/decoder/metadata records. All have actual frozen RED before code. Success logic was not redesigned. Mechanical fallout: existing spool copy pub(super) for history and existing manager control error adapter pub(super) for archive; fixture-only resource-loader/visible oracle-type reuse. No new carrier or fallback. Main approved exact parking_lot=0.12.5 only for reentrant RetainedControlDb; Cargo.lock and [dependency purpose](../dependencies.md) are tracked.

Formatting passes before every commit. Storage cargo check passes. Warning-mode storage/test Clippy passes with zero owned changed-line diagnostics; current75 storage and521 consolidated fixture/dependency diagnostics are inherited outside owned changed lines. Main explicitly does not treat those as slice blockers. Protected strict no-emit checks and Biome pass for new/changed archive, terminal and mailbox fixtures; the formatted legacy session-storage fixture retains7 pre-existing any warnings. Manual changed-line Rust/TS readability review completed; numerical analyzer unavailable.

Exact revision/command ledger: `/tmp/storage3-proof-ledger.md`; full outputs `/tmp/storage3-*.log`, Clippy JSONL and changed-line diagnostic JSON. History filtering's initial manager-load rejection and terminal fixture type/1-vs1.0 failures were fixture defects, not production REDs; current assertions retain every JS value/field. Existing90s spool proof was not rerun merely for this checkpoint.

## Remaining integration scope

No assigned module remains open. Session-manager belongs to agent_293 (`repin/session-manager`), not this lane. Final workspace integration, independent acceptance and inherited lint debt remain parent-owned. Current compact persistence8 is `/tmp/storage3-control-light-reciprocal-valid.log` (suite_core_4); the earlier suite_session_1 filter selected zero cases and is not proof. Native routing1/reentry1/reference-wire1 pass; protected strict ESM types pass at `/tmp/storage3-control-mts-types.log` with unchanged compiler options. Workspace all-target check and formatting pass; final Clippy logs are `/tmp/storage3-control-close-clippy-{storage,fixtures}.{jsonl,log}`.

Gap: adversarial/wrong-typed shapes, exceptional OS/fsync faults, numeric extremes and runtime races follow the current parity exclusions; no new fault-hunting or scheduling/carrier work was introduced. This is not injected crash-recovery acceptance.
