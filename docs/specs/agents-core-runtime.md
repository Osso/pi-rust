# Agents-core runtime

The orchestration runtime follows `extensions/agents-core/src/runtime.ts` at frozen `4f9e6f8a4`. Its [module trace](../porting/traceability/agents-core.md) maps source declarations, branches and proof.

## What it must do

### Session construction

- [x] Child and attached factories forward the parent model registry and configured model, effort, context and extension factories. Goal extensions are excluded.
- [x] Child tools include ask_questions/contact_parent/send_agent_message/end_turn when a profile restricts tools. Browser profiles also include read. Children cannot use change_working_directory or supervisor-only tools.
- [ ] Browser read definitions use the supplied configuration root, or the module-import HOME snapshot. Reads outside that root fail.
- [x] Attached factories restore archived transcripts, validate their session identity and cwd, bind extensions, and leave settings creation to SDK. They do not apply a separate permission override.

### Admission and recovery

- [x] Explicit unknown profiles fail with frozen guidance before persistence admission. Child model metadata uses the configured or parent model and model-clamped configured effort, defaulting to medium independently of parent effort.
- [x] Attach-by-path restores archives and reads display names from control DB metadata. Parentless attachments use on-request permission.
- [x] Runtime recovery does not depend on parent agent journal entries. Production spawn and completion write no legacy parent agent journal entries.
- [x] Child construction and transcript errors fail explicitly. Shutdown and disposal run in source order; a child shutdown does not invalidate shared parent dispatches.

### Coordination and lifecycle

- [x] Wait slices capture the latest foreground model-request deadline without cancelling active agents. Captured agent order selects terminal wakes.
- [x] Coordination wakes preserve their prompt. Wait tools return claimed mailbox references for durable session acceptance; they do not mark content delivered first. Bridge waits release claims because their return value is not accepted session input.
- [x] Parent routing uses runtime parent identity, not a legacy transcript-header lookup. Mailbox result text preserves the target and exact frozen failure messages. Bridge messages.send does not run the tool-only session callback.
- [x] Detached terminal notifications use the frozen suppression predicate rather than an extra transport scan. Cancellation respects its settlement timeout and does not cancel the dispatch on timeout.
- [x] Each store uses its factory agentDir for terminal artifact delivery and dead-detached reconciliation. Descendant settlement and detached cleanup remain ordered before terminalization.

## How it works

- [Frozen symbol/branch trace and proof ledger](../porting/traceability/agents-core.md).
- [Wait slice](agent-wait-slice.md), [mailbox tools](agent-mailbox-tools.md), [runtime acceptance](../porting/traceability/runtime-mailbox-acceptance.md).

## Implementation inventory

- `crates/pi-coding-agent-extensions/src/bundled_extensions/agents_core/runtime.rs` — public request routing, runtime handles, formatting and artifact roots.
- `session_factories.rs`, `spawn_attach_runtime.rs` — child construction, profile admission, attachments and recovery.
- `dispatch_runtime.rs`, `control_runtime.rs`, `lifecycle_runtime.rs` — dispatch lifetime, cancellation, steering and terminal publication.
- `wait_runtime.rs`, `mailbox_runtime.rs`, `registration.rs`, `viewer.rs` — wait/claim semantics, messaging, hooks, commands and listing.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/agents_core_session_factories.rs` — production factory differential, archive/metadata, profile/model and browser-read boundaries.
- `crates/pi-coding-agent/tests/agents_core_runtime.rs` — persistent dispatch/recovery, coordination receipts, callback scope, mailbox messages and shutdown.
- `crates/pi-coding-agent/tests/agents_core_drift.rs`, `agents_core_end_to_end.rs` — accessed-field errors, real frozen runner lifetime, real child provider/session and no parent journal.
- `crates/pi-coding-agent/tests/differential_agents_core_deterministic.rs`, `terminal_outbox_delivery.rs` — exact output/metadata, empty filters and artifact-root cleanup.
- `wait_runtime.rs::tracked_read_failure_tests` — persisted error wakes and ordered terminal selection.

## Known gaps (current cycle)

None within the approved user-observable parity scope. See the module trace for standing native-extension/runtime exclusions.

## Out of scope

Live provider/network validation, deployment and storage-internal redesign. Fault/pathological-input and scheduling exclusions follow repository policy; frozen source files remain read-only.
