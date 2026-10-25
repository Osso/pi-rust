# Goal extension (`extensions/goal/src`)

Port group: `crates/pi-coding-agent-extensions/src/bundled_extensions/goal/`.
Registered as `<first-party:goal>` through `core::extensions::loader::ExtensionRegistry`
(`bundled_extensions::register_bundled_extensions`).

The extension is a factory over the Rust `ExtensionApi`; the same events, tool,
command, renderers, and message/status payloads as the TypeScript source are
registered. TypeScript file → Rust module mapping is one file per module with
the source basename converted to snake_case.

## Re-pin fallibility boundary

Frozen `4f9e6f8a4` synchronous settings errors now propagate through the native error-status scheduler and idle-goal selection callbacks. Exact APIs, branch distinctions and currently unproven failure-injection cases: [native fallibility propagation](retry-delay.md#native-fallibility-propagation). The management, scheduling, review and owner-renderer differentials below now cover this boundary against the common pin.

## Symbols and branches

| Source symbol / branch | Rust item | Tests |
|---|---|---|
| `parseGoalArgs`, unsupported flags, bare objective, control actions | `goal_args::parse_goal_args` | `goal_extension::parses_goal_args_like_the_command_parser`; differential `parse-args` |
| `parseGoal`, `parseGoalJson`, evidence filtering | `goal_parsing::{parse_goal, parse_goal_json}` | `parses_goal_json_and_drops_invalid_fields`; differential `parse-goal`, `parse-goal-json` |
| `deduplicateCurrentGoalScope` | `goal_objective::deduplicate_current_goal_scope` | `deduplicates_restated_goal_scope`; differential `dedupe` |
| `goalFooterStatus`, `goalStartupMessage`, `goalViewMessage`, `goalSystemBlock` | `goal_presentation::*` | `presents_goal_status_and_system_block`; differential `present` |
| `findLastAssistantMessage`, abort/empty classification | `goal_turn::*` | `classifies_the_last_assistant_turn`; differential `turn` |
| `isStaleContextError` | `stale_context::is_stale_context_error` | used by the empty-response and error-status schedulers; behavioral coverage below |
| `loadOrMigrateGoal/Active/Running`, legacy migration, save/pause/resume/complete/clear | `goal_state::*` | command/tool tests in `goal_extension.rs`; in-memory and legacy paths |
| `Goal` JSON round-trip and field order | `goal_types::Goal` | differential `matches_typescript_goal_goal_value_serialization` |
| `createGoalReviewEvidenceController` append/consume/review | `goal_review_evidence::*` | `review_evidence_accumulates_and_is_sent_to_completion_review` |
| `withSupervisorReviewStatus`, resident review | `supervisor_review::*` | wrapper returns an `error` decision on review failure (traceability note: resident client is shared with `supervisor::client`) |
| `parseGoalArgs` command dispatch, set/view/pause/resume/clear | `index::handle_goal_command` | command tests |
| `manageGoal` set/pause/resume/complete/clear/status | `index::manage_goal` | tool tests |
| Supervisor decision handling (idle + completion) | `index::apply_goal_idle_decision`, `apply_completion_decision` | agent-end, completion, and wait tests |
| Evidence scheduling, completion scheduling, empty-response retry, error-status settlement | `completion_scheduling`, `empty_response_scheduling`, `error_status_scheduling`, `goal_scheduling` | `idle_review_with_wait_schedules_a_five_minute_review_status`, `emits_abort_and_error_skips_without_reviewing`; non-stale timer errors reach the process-level uncaught handler ([throw-audit](throw-audit.md)) |
| Session start/fork inheritance, shutdown, input/tool-result evidence, agent start/end, before-agent-start injection | `index::register_*` | `session_start_restores_running_goal_and_requests_continuation`, `injects_the_running_objective_into_the_system_prompt` |
| Supervisor status/message renderers, instruction wrapper, countdown formatting | `rendering::*` | module tests `wraps_and_unwraps_supervisor_instructions`, `formats_review_countdown`; behavioral status tests |
| Countdown refresher | `wait_countdown::*` | exercised through status append/restore paths; timer behavior is OS-time dependent and not differentially compared |
| `appendGoalSchedulingError`, `createWaitStatusCallbacks` | `wait_status::*` | used by wait tests |
| `registerManageGoalTool` schema/description | `goal_tool::*` | `registers_manage_goal_and_goal_command_without_budget_fields` |

## Supporting core changes

- `core::session_manager::SessionManager` gained the TS goal accessors
  (`setMetadataControlDbPath`, `getMetadataControlDbPath`,
  `getSessionGoalJson`, `getSessionGoalJsonForSession`,
  `setSessionGoalJson`, `clearSessionGoalJson`) backed by the existing
  `session_control_db` goal row. The metadata snapshot preserves the goal row,
  matching `writeMetadataSnapshot`.
- `core::event_bus::EventBus` requires `Send + Sync` so extension API values can
  be captured by the async handlers/timers the TS extension uses. This is a Rust
  concurrency requirement, not a semantic change.

## Gaps / discrepancies

- Timing behavior (1s retry, 5min wait review, 60s Supervisor deadline,
  countdown redraw cadence) is implemented with `tokio` timers. The source
  tests use fake timers; Rust tests cover the observable scheduling decisions
  with real short waits instead of asserting exact wall-clock ticks.
- `GoalExtensionOptions::review_goal` is the Rust seam replacing the TS optional
  `reviewGoal`; the resident Supervisor path calls the shared
  `supervisor::client`/`project_resolver` modules.
- Rendering uses `pi_tui` components; component-factory overloads are not part
  of the goal source and were not added.

## Re-pin management audit (2026-10-25)

The complete frozen index module was compared to the native registration,
management and event paths. `goal_management_outputs_match_frozen_without_extra_notifications`
runs eleven concrete tool cases through both installed extensions: status, pause
validation/success, resume without paused state, clear, completion accept/continue/
pause/error, and goal-set accept/reject. It compares complete tool content/details,
notifications and persisted status messages. RED: native status emitted an extra
notification; GREEN after removing all old tool-only notifications and including
continue instructions in the persisted rejection status. Idle and command notifications
remain as frozen specifies. `goal_commands_propagate_notification_errors` proves
all six command paths return the original UI failure rather than silently succeed.
Existing behavior tests cover command validation/dispatch, running-goal injection,
review evidence, continuation suppression, startup, idle decisions and footer state.

| Excluded branch | Classification |
| --- | --- |
| Node process/stdio fault shapes, pathological JSON and scheduler interleavings | gap: repository parity policy; not pursued |

Proof: `/tmp/bundled3-goal-index-green2.log`, 31 focused tests passed.
