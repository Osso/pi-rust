# Reminders

This is the Rust port contract for TypeScript revision `be6413897c72efe708232c1a139ce01815d9cd18`. The implementation is in progress; see [traceability](../porting/traceability/reminders.md) for proof status. Reminders are machine-local follow-up context, not a task list or authorization to act.

## What it must do

### Global persistence and lifecycle

- [ ] Store reminders in the Pi-owned control SQLite database, shared by local sessions and projects for one OS user; project path is optional context and never a list filter. Records survive session completion and restart.
- [ ] Persist an autoincrement ID, nonblank title, notes (default empty), optional project path, UTC due instant, `open`/`done` status, creation time, optional completion time, and optional delivery acknowledgement. New reminders are open. Sort lists by due instant then ID.
- [ ] Complete sets status to done and preserves an existing completion time; reopen sets open and clears completion and delivery times. Editing or rescheduling does not reopen. A changed due instant clears delivery acknowledgement; an unchanged instant or other edits preserve it. Delete removes the record. Reject missing IDs, invalid IDs, empty titles and empty edits.
- [ ] Treat reminders as context only: viewing one neither starts nor completes work. Agents may add useful future follow-ups without being asked and must report the ID and due date and time. Active work belongs in `PLAN.md`.

### Interfaces

- [ ] Provide the `reminders` tool with actions `list`, `add`, `edit`, `reschedule`, `complete`, `reopen`, `delete`; optional fields are `id`, `title`, `notes`, `projectPath`, `dueAt`, and list `status`. Add requires title and dueAt; mutations require ID; reschedule requires dueAt; edit changes supplied fields. Default list status is `open`; `done`, `all`, and `due` are available. `due` includes overdue open reminders and those due before the next local midnight, not completed reminders.
- [ ] Provide `/reminders [list [open|done|all|due]]`, `add <due> <title>`, `edit <id> <title>`, `notes <id> <text>`, `reschedule <id> <due>`, `done <id>`, `reopen <id>`, and `delete <id>`. Empty command lists open reminders. Parse local date and clock as two words or an ISO timestamp as one word. Report command errors through the UI.
- [ ] Match the frozen result text: `#<id> [<status>] due <local time>: <title>`, optional ` (project: <path>)`, followed by indented notes; list results use newline separators. Empty lists say `No <status> reminders.`; mutations prefix the formatted record with `Added reminder`, `Edited reminder`, `Rescheduled reminder`, `Completed reminder`, or `Reopened reminder`; delete says `Deleted reminder #<id>.` Missing control DB reports `Reminders require the Pi control database.`

### Time

- [ ] Accept exact local `YYYY-MM-DD HH:mm[:ss]` or `YYYY-MM-DDTHH:mm[:ss]`, and T-separated ISO times with `Z` or `±HH:MM` and optional seconds and up to three fractional digits. Reject malformed or invalid calendar values. Resolve relative dates before calling the tool and ask for clarification when needed.
- [ ] Interpret unzoned input in the machine timezone; reject nonexistent local times and repeated local times at clock transitions rather than guessing. An explicit offset selects an instant. Store UTC instants and display in the current local timezone as `YYYY-MM-DD HH:mm:ss GMT<offset> [<zone>]` (the zero-offset representation follows the frozen formatter). A timezone change must not change the stored instant.

### Delivery

- [ ] Only the session whose ID matches `reminders.deliverySessionId` and which has a control DB runs delivery. Start checks one minute after session start, not immediately, then every minute; stop on shutdown. This permits an interrupted turn to resume before a reminder turn starts. No configured or running delivery session means no automatic delivery.
- [ ] Each check selects open, due, unacknowledged reminders, in due/ID order, excluding reminders already pending in this process. For each, attempt a desktop notification (`Pi reminder`, `#<id> <title>`) and send a visible `reminder` custom follow-up message with a fresh delivery token and a triggered assistant turn. The message states that the reminder is context, not authorization.
- [ ] Acknowledge only after the token appears in a written conversation entry, and only if the reminder remains open, unacknowledged and due at the same instant. Check acknowledgements on each timer tick and at `agent_end`. At `agent_end`, release unwritten pending messages so the next check can retry; exit loses pending state, allowing retry after restart. A crash after the conversation write but before database acknowledgement can cause duplicate delivery and alerts.
- [ ] Log failed desktop alerts without retrying the alert itself; conversation acknowledgement does not depend on alert success. Report delivery-check errors through the UI. Reopening or changing the due instant re-arms delivery.

## How it works

- [Frozen source and proof map](../porting/traceability/reminders.md).

## Implementation inventory

- Rust storage, time, actions, interface and delivery are implemented; full runtime integration proof is pending.
- TypeScript sources and tests are listed in the traceability map.

## Tests asserting this spec

- Focused Rust behavioral and bidirectional persistence tests are recorded in traceability. Full runtime delivery proof is pending.

## Known gaps (current cycle)

- [ ] Full runtime delivery, notification and crash-boundary proof remains pending.
- [ ] The frozen feature does not supervise or restart the delivery session; overdue reminders wait for its next start.
- [ ] Failed desktop alerts are not retried independently.

## Out of scope

- Cross-machine synchronization, recurrence, task ownership, and automatic work authorization.
