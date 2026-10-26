# Reminders traceability — proof pending

Authorized delta source: `be6413897c72efe708232c1a139ce01815d9cd18`, not the common pin `4f9e6f8a4`. Paths below are relative to the TypeScript checkout at that revision. Contract: [reminders spec](../../specs/reminders.md). Test coverage below is based on source inspection, not a test run in this audit. Passing results are claimed only where recorded in the proof ledger; full parity remains unverified.

| Frozen source / behavior | Rust coverage | Required proof |
|---|---|---|
| `src/core/session-control-db.ts`: Pi-owned `reminders` table, autoincrement ID, title/status/completion constraints and nullable delivery state | `session_control_db/schema.rs` | Additive v15 schema and bidirectional TS/Rust fixtures. |
| `src/core/reminder-store.ts`: create, sorted list, patch, complete, reopen, delete, due query and conditional acknowledgement | `session_control_db/reminders.rs` | Five storage tests cover lifecycle, due-time match, completion, re-arming and bidirectional TS/Rust rows. |
| `src/core/reminder-time.ts`: strict local/offset parsing, DST gap and fold rejection, UTC canonicalization, local display and due-today boundary | `crates/pi-js-compat/src/reminder_time.rs` | Focused frozen differential across UTC, Chicago, Lord Howe, Apia and US/Central; all pinned Node-accepted tzdata IDs checked for display names. Full module audit remains pending. |
| `extensions/reminders/src/actions.ts`: validation, list filters, result formatting | `bundled_extensions/reminders/actions.rs` | Four native behavioral tests cover validation, CRUD, due filtering, result text and UTF-16; tool and command integration remains pending. |
| `extensions/reminders/src/index.ts`: tool schema and guidance, command parser and UI notification, DB requirement, lifecycle registration | `bundled_extensions/reminders/{interface,registration}.rs` | Native interface tests, default-process tool registration, and extension-runner tool and command execution. |
| `extensions/reminders/src/delivery.ts`: configured session only; delayed first check, periodic due selection, notification and custom follow-up, token tracking, conversation-entry acknowledgement and pending release | `bundled_extensions/reminders/delivery.rs` | Delivery-state JSONL/CAS, extension-runner gating/timer/follow-up/notification, and real RPC process kill/restart tests. As in the frozen TS test, the pre-kill alert and streaming state do not directly prove custom-queue insertion. |
| `src/core/settings-manager.ts`: `reminders.deliverySessionId` setting | `SettingsManager::get_reminders_delivery_session_id` | Native and bidirectional settings fixtures and extension-runner session gating. |

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

## Reminder timezone names

`crates/pi-js-compat/src/reminder_time_zones.rs` contains all 597 Node-accepted IDs from system tzdata `2026e`, including canonical IDs as identity entries and aliases mapped to Node Intl canonical names. Generated with Node `26.12.1`, ICU `78.3`, and Node tzdata `2026a`; system tzdata and Node tzdata are not the same version. Node-rejected tzdata IDs are excluded. A zone that cannot be identified or an ID absent from the table returns an explicit error, not a UTC or identity fallback.

To refresh, run `python3 scripts/test-sandbox.py -- node oracle/reminder-time-zones.mjs` and replace the table with its output, then run the focused reminder-time tests with the Cargo sandbox runner. Update the version assertions and this note with the new reference versions. The exhaustive test directly compares every table entry with Node and exercises formatted output through Jiff. It requires the recorded ICU, Node tzdata and system tzdata versions, but not the Node patch version. The generated Node version remains provenance. Mismatched inputs or mappings fail rather than silently accepting stale data.

A copied `/etc/localtime` without `TZ` is matched against zoneinfo using ICU's first-match traversal; native process tests compare Chicago and Paris with Node. Approved gap: IDs or aliases introduced after the pinned inputs are not recognized until the table is refreshed. The table pins display names, not timezone transition data; Jiff and Node may have different tzdata releases.

## Proof ledger

- `cargo test -p pi-js-compat --lib reminder_time::tests -- --nocapture`: five passed, including the 597-ID Node comparison, delta differential and copied-localtime cases.
- `cargo test -p pi-coding-agent-storage --lib reminder_ -- --nocapture`: five passed, including bidirectional TS/Rust persistence.
- `cargo test -p pi-coding-agent --test suite_extensions reminders_runtime::reminders_runtime -- --exact --nocapture`: one passed, covering tool, command, timer, session gate, follow-up and notification through the extension runner.
- `cargo test -p pi-coding-agent --test suite_core_4 reminder_process_crash::queued_reminder_survives_kill_and_restart -- --nocapture`: one passed independently on each host, exercising real RPC processes, SQLite, JSONL, fake provider and notification.
- Frozen limitations: no delivery-session restart supervision; failed desktop alerts are not independently retried. Node/runtime pathological inputs and scheduling races follow the repository's recorded parity bar.
