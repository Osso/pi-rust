# Fallible UI notifications and event listeners

Frozen `core/extensions/types.ts`, `core/event-bus.ts:19-23`, and their callers at `4f9e6f8a4` govern these boundaries.

## What it must do

- [x] Return `Result<(), CallbackError>` from native UI notifications. Preserve native Error branding/properties and arbitrary JSON throws through callers.
- [x] Event and request listeners retain `CallbackError`; event dispatch logs the original throw and continues later listeners. Native failure diagnostics retain the original error, not only its message.
- [x] Ordinary callers propagate notification failures. Keep each frozen local catch boundary: read-log retries its notice after read or first-notice failure; DOOM formats String(error) inside its catch; background-dispatch notice failure escapes its unhandled continuation.
- [x] Queued question notification failures reach the frozen outer delivery catch without creating a failure mailbox record after that throw.

## How it works

- [Event bus map](../porting/traceability/event-bus.md).
- [Question tool map](../porting/traceability/ask-questions.md).
- [Compaction callback prerequisite](extension-compact-callbacks.md).

## Implementation inventory

- `crates/pi-coding-agent-core/src/core/extensions/types.rs`: UI result contract.
- `crates/pi-coding-agent-core/src/core/event_bus.rs`: callback error transport and catch logging.
- `crates/pi-coding-agent-core/src/core/desktop_notification.rs`: shared original-throw console reporter.
- Native bundled extensions, examples, host UI adapters and test hosts implement the same result boundary.

## Tests asserting this spec

- `tests/fallible_notifications.rs`: Error/code/raw/null UI rejection retention.
- `tests/differential_event_bus_errors.rs`: actual frozen catch arguments versus native original thrown values.
- `tests/event_bus.rs`: listener delivery, recovery, request claims and errors.
- `tests/differential_question_queue.rs`: actual-source/native child history proves original raw notification throw logging, no failed delivery record and FIFO recovery to the captured session.

## Known gaps (current cycle)

- Workspace all-target compile is clean (0 errors/warnings); local-catch consumer regressions and remaining final gates are pending. This is not a whole-question-tool completion claim.

## Out of scope

- Other UI API migrations, provider calls, live desktop state, and independent background-dispatch promise rejection behavior.
