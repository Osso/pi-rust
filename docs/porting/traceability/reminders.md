# Reminders traceability — proof pending

Authorized delta source: `be6413897c72efe708232c1a139ce01815d9cd18`, not the common pin `4f9e6f8a4`. Paths below are relative to the TypeScript checkout at that revision. Contract: [reminders spec](../../specs/reminders.md). This document does not assert that Rust implementation or parity exists. All Rust proof below is **pending**; TypeScript tests identify reference behavior, not proof of a Rust port.

| Frozen source / behavior | Rust coverage | Required proof |
|---|---|---|
| `src/core/session-control-db.ts`: Pi-owned `reminders` table, autoincrement ID, title/status/completion constraints and nullable delivery state | Pending | Bidirectional TS/Rust control-DB fixtures, schema and existing-DB creation/compatibility |
| `src/core/reminder-store.ts`: create, sorted list, patch, complete, reopen, delete, due query and conditional acknowledgement | Pending | Bidirectional persistence plus lifecycle, due-time match, completion and re-arm behavior |
| `src/core/reminder-time.ts`: strict local/offset parsing, DST gap and fold rejection, UTC canonicalization, local display and due-today boundary | Pending | Native behavioral tests against frozen behavior, including timezone changes and DST |
| `extensions/reminders/src/actions.ts`: validation, list filters, result formatting | Pending | Tool and command behavioral tests for all actions, errors and concrete output |
| `extensions/reminders/src/index.ts`: tool schema and guidance, command parser and UI notification, DB requirement, lifecycle registration | Pending | User-visible tool/command tests and session integration |
| `extensions/reminders/src/delivery.ts`: configured session only; delayed first check, periodic due selection, notification and custom follow-up, token tracking, conversation-entry acknowledgement and pending release | Pending | Runtime behavior and persisted-message proof, including busy queue, abort, restart and crash boundary |
| `src/core/settings-manager.ts`: `reminders.deliverySessionId` setting | Pending | Settings persistence/interoperability and matching versus nonmatching sessions |

## Frozen details for porting

- The table has `id`, `title`, `notes`, `project_path`, `due_at`, `status`, `created_at`, `completed_at`, `delivered_at`; completion consistency is constrained in SQLite. Due and creation timestamps are UTC ISO strings. `delivered_at` is not a completion marker.
- `edit` can patch title, notes, project path and due time at the action layer; the public tool schema exposes a string project path, while the internal action type also permits null to clear it. `notes` command edits notes, including an empty value. A same-instant reschedule does not clear `delivered_at`; reopen always clears it, including when already open.
- Timer delivery calls `notify-send --app-name=pi` before sending the message. Each attempt may alert again after an unwritten message is released. Alert failures are logged, not separately retried. `agent_end` drops only unwritten pending tokens; acknowledgement additionally checks open status, matching due time and absent prior acknowledgement. A written entry followed by a crash before acknowledgement can be delivered again.
- The initial timer tick is delayed to avoid displacing interrupted-turn resume. Delivery checks use `now` and do not initiate model calls unless a reminder is sent. Process-local pending state does not survive shutdown.

## Reference tests at the delta

- `packages/coding-agent/test/reminder-store.test.ts`: DB constraints, CRUD, filters, acknowledgement and re-arming.
- `packages/coding-agent/test/reminder-time.test.ts`: parsing, offset, DST and local display.
- `packages/coding-agent/test/suite/reminders.test.ts`: tool, command, cross-session visibility and delivery behavior.
- `packages/coding-agent/test/suite/regressions/reminder-delivery-crash.test.ts`: actual process kill, restart/resume ordering, conversation and `notify-send` path.

## Proof ledger

- Documentation-only source audit at the locked TS revision. No Rust tests or bidirectional fixtures were run for this document; all port rows remain pending.
- Frozen limitations: no delivery-session restart supervision; failed desktop alerts are not independently retried. Node/runtime pathological inputs and scheduling races follow the repository's recorded parity bar.
