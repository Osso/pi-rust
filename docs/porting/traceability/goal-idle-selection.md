# Goal idle selection traceability

Source: `packages/coding-agent/extensions/goal/src/goal-idle-selection.ts`, common pin `4f9e6f8a4`. Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/goal/goal_idle_selection.rs`.

| Frozen export / branch | Native proof |
|---|---|
| `selectGoalForIdleReview`: absent goal returns before pending check | ten-case real frozen differential, null-goal case |
| pending messages clear retry and return before all terminal-status handling | pending normal/empty/error/aborted cases |
| aborted clears/reports exact message | no-pending aborted case |
| error clears/schedules exact status | no-pending error case |
| empty assistant schedules retry | no-pending empty case |
| nonempty/no assistant clears and returns goal | normal and no-assistant cases |

Test: `suite_extensions::goal_idle_selection::goal_idle_selection_matches_frozen_pending_and_terminal_order`; oracle: `oracle/goal-idle-selection-oracle.ts`. Results and observable callback order compare against the real unmodified frozen export. RED: `/tmp/bundled-goal-idle-red.log` shows the old port omits the pending-first gate even for normal terminal turns. The old pending-input status message is deleted, not retained as alternate behavior.

Gap: pathological inputs/runtime fault shapes follow the shared parity exclusions. The containing index row owns live context callback and persistence wiring.
