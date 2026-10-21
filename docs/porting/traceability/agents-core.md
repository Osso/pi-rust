# agents-core traceability

## Foreground wait slice refresh

Frozen runtime.ts:196,2126-2295,3157-3168 maps to shared `latest_model_request_started_at`, model-request-start and agent/session-end hooks, and the captured deadline in native wait_runtime.rs. `wait_slice_uses_latest_model_request_and_does_not_cancel_active_agents` uses a real SQLite active child and paused Tokio timers: timedOut true, child still running, agent_end clears the deadline. Model-request-start/end events now exist in extension types/runner and AgentSession forwarding. [Contract and remaining event-boundary proof](../../specs/agent-wait-slice.md).

## Frozen 4f9e6f8a4 refresh — 2026-10-21

Historical module inventory below is not renewed source correspondence. Frozen `runtime.ts` now owns private `emptyAgent` (3065), `isInFlightLifecycle` (2526), result helpers (391-412), prompt validators (424-455), `listAgents` (1875) and wait validation (993). The deterministic oracle loads an in-memory transpiled copy and exposes those declarations without editing frozen files; viewer now loads `extensions/agent-viewer/src/runtime.ts:85`.

Frozen `runtime.ts:402-412` and agent-viewer result constructors omit `isError`; native `error_tool_result` now does the same. Explicit per-call-site flags remain separate behavior. Refreshed deterministic oracle reproduced the prior `true` mismatch.

Retired differential cases: generic `createFailedMailboxMessage` and `appendParentAgentStart` are absent from frozen non-test source; removed `failed-message` and `journal` cases are not new-pin parity evidence. Native legacy helpers/tests remain outside this bounded oracle repair. Registration still invokes real frozen exported registration functions.

Current proof (2026-10-21 question lane): deterministic cases and full frozen registration differential pass, including 12/12 exact metadata and set_agent_model ordering. Child-aware ask_questions/answer_question are implemented with durable tool-result receipt acceptance; [relay traceability](question-relay-tools.md) tracks remaining branch proof. toHost canonical routing, historical viewer trace and 25-minute wait slice are now ported with focused proof; actual host-relay transport, shared lossless TextContent/details and whole-runtime gaps remain, so metadata equality is not whole-runtime parity.

Runtime/store/lifecycle old-clock failures were expired canonical rows: frozen `multi-agent-store.ts:939-945` persists the store row before runtime enqueue (`runtime.ts:2954-3040`). Fixtures now use current birth times. Child-disabled capability includes frozen `answer_question`; `set_agent_model` was already registered natively and only missing from the old fixture expectation.

Source: `packages/coding-agent/extensions/agents-core/src/*.ts` (pinned by
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/bundled_extensions/agents_core/`.

Proof commands:

- `cargo test -p pi-coding-agent --test agents_core_shared_viewer` (16)
- `cargo test -p pi-coding-agent --test agents_core_runtime` (17)
- `cargo test -p pi-coding-agent --test differential_agents_core_deterministic` (2)

Differential oracle: `oracle/agents-core-deterministic-oracle.ts` (33 cases:
snapshots, lifecycle predicates, prompt validation, failed mailbox messages,
tool results, list/viewer text+details, journal records, wait-parameter
validation, and full tool/command metadata).

## shared.ts

| Source symbol | Rust item | Proof |
| --- | --- | --- |
| `CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE` | `shared::CHILD_ORCHESTRATION_UNAVAILABLE_MESSAGE` | runtime/spawn tests |
| `fileReferenceSchema` | `shared::file_reference_schema` | tool metadata differential |
| `WaitAgentsWakeUp` | `shared::WaitAgentsWakeUp`/`WaitAgentsWakeKind` | wait tests |
| `emptyAgent` | `shared::empty_agent` | differential `empty` |
| `isChildAgentRuntime` | `shared::is_child_agent_runtime` | child-runtime spawn test |
| `isInFlightLifecycle` | `shared::is_in_flight_lifecycle` | differential lifecycle cases |
| Factory contracts (`ChildAgentSession`, `ChildAgentSessionFactory`, `AttachedSessionFactory`) | `shared.rs` (moved from `spawn-attach-runtime` to break the Rust module cycle) | spawn/attach tests |

## tool-results.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `toolResult` | `tool_results::tool_result` | differential `tool-results` |
| `errorToolResult` | `tool_results::error_tool_result` | differential `tool-results` |
| `emptyToolResult` | `tool_results::empty_tool_result` | differential + wait test |

## runtime-utils.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `abortAgentHandleSafely` | `runtime_utils::abort_agent_handle_safely` | cancel test |
| `createFailedMailboxMessage` | `runtime_utils::create_failed_mailbox_message` | differential `failed-message` |

## viewer.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `listAgents` | `viewer::list_agents` (+ `ListAgentsParams`) | differential list cases, runtime request-handler list test |
| `agentViewer` | `viewer::agent_viewer` (+ `AgentViewerParams`) | differential viewer cases, persisted-session behavioral case |
| formatting/status/commands/transcript helpers | private fns in `viewer.rs` | differential + `agents_core_shared_viewer` |

Persisted viewing uses `list_session_metadata` + `read_multi_agent_state`, matching the source.

## parent-agent-journal.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `isTerminalAgentLifecycle` | `parent_agent_journal::is_terminal_agent_lifecycle` | dispatch tests |
| `appendParentAgentStart/Completion`, private record writer | `append_parent_agent_start/completion`, `append_parent_agent_record` | differential `journal`, runtime spawn test |
| `readActiveParentAgentIds` (+ entry scan) | `read_active_parent_agent_ids`, `read_active_parent_agent_ids_from_entries/session_entries` | entry-scan test |
| `registerParentAgentCompactionRefresh` | `register_parent_agent_compaction_refresh` | implemented; not separately exercised (requires compact event emission) |
| `restoreCompactedParentAgentRecords` | `restore_compacted_parent_agent_records` | no-op path test |

## session-factories.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `MAX_GOAL_OBJECTIVE_CHARS` | `session_factories::MAX_GOAL_OBJECTIVE_CHARS` | differential |
| `validateGoalObjective` | `validate_goal_objective` | differential blank/trim/exact/oversized/emoji |
| `spawnPromptValidationMessage` | `spawn_prompt_validation_message` | differential |
| `backgroundPromptValidationMessage` | `background_prompt_validation_message` | differential |
| `isProductionChildSessionFactory` | `is_production_child_session_factory` + factory marker | spawn validation test |
| `createProductionChildAgentSessionFactory` | `create_production_child_agent_session_factory` | 12 factory tests + end-to-end spawn |
| `createProductionAttachedSessionFactory` | `create_production_attached_session_factory` | attach open/policy/mismatch tests |
| `resolveInheritedLeafId` | `resolve_inherited_leaf_selection` | inherited branch tests |
| `createChildSessionManager` | `create_child_session_manager` (+ `create_child_session_manager_options`) | fork/leaf selection tests |
| `forkInheritedChildSessionManager` | forked inside `create_child_session_manager` | inherited prefix test |
| `createChildAgentSessionOptions` | `create_child_agent_session_options` | option/identity/tool tests |
| `createBoundChildSession` | `create_bound_child_session` | prompt/bind/persist tests |
| `createProductionChildSession` | `create_production_child_session` | fresh/inherit + goal-validation tests |
| `createProductionAttachedSession` | `create_production_attached_session` | attach tests |
| `openAttachedSessionManager` | `open_attached_session_manager` | attach tests |
| `createAttachedSettingsManager` | `create_attached_settings_manager` | attached policy test |
| `validateAttachedSessionIdentity` | `validate_attached_session_identity` | mismatch tests |
| `resolveChildAgentProfile`/`Tools` | `resolve_child_agent_profile`, `resolve_child_agent_tools` | profile/tool order tests |
| `resolveChildExtensionFactories` | `resolve_child_extension_factories` | static/dynamic factory tests |
| `createSessionStartEvent` | `create_session_start_event` | fork/resume event assertions |
| `getSessionTranscriptMetadata` | built inline in `create_bound_child_session` | transcript assertions |
| `findActiveToolCallEntry` | `find_active_tool_call_entry` | active-tool-call tests |
| `findUnresolvedTurnParentId` | `find_unresolved_turn_parent_id` | inherited leaf tests |
| `validatePersistedInheritance` | `validate_persisted_inheritance` | unpersisted-parent test |

The factory tests cover fresh and inherited child sessions, attached sessions,
goal-extension exclusion, and the child control-tool list. The end-to-end test
(`agents_core_end_to_end`) spawns a real child through the production factory
against a faux provider and a temporary control DB.

## lifecycle-runtime.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `createLifecycleCoordinator` | `lifecycle_runtime::create_lifecycle_coordinator` | spawn/cancel tests |
| `publishCoordinatorSnapshot` | `publish_coordinator_snapshot` | spawn completion, cancel, wait tests |

Rust takes `&Arc<MultiAgentStore>` because terminal-outbox delivery needs an
owned store handle. Process identity is parsed once from
`get_runtime_process_instance_id()` (source parses at module load).

## mailbox-runtime.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `sendAgentMessage` (+ validation/delivery helpers) | `mailbox_runtime::send_agent_message` | mailbox delivery test (child session + runtime mailbox row) |
| `contactParent` | `contact_parent` | contact-parent test |
| `createRuntimeLifecycleMirror` | `RuntimeLifecycleMirror` | used by registration/request handler |
| `mirrorPendingLifecycleRuntimeMailboxMessages` | `mirror_pending_lifecycle_runtime_mailbox_messages` | wait/registration path |
| `currentMessageSenderId` | `current_message_sender_id` | steer identity test |
| `registerMailboxTools` | `register_mailbox_tools` | tool metadata differential |
| schemas | `send_agent_message_schema`, `contact_parent_schema` | tool metadata differential |

Error text, failed-message fallbacks, recipient resolution, detached terminal
transport de-duplication, and store-ref checks match the source.

## control-runtime.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `cancelOwnedAgentRuntime`/`cancelAgent` | `cancel_owned_agent_runtime`, `cancel_agent` | cancel test |
| `wakeWaitAgentsAfterSteering` | `wake_wait_agents_after_steering` | implemented; listener path exercised by wait watcher |
| `requestInteractiveAgentSteering`/`requestAgentSteering` | same names | unknown-agent steering test |
| `steerAgent` | `steer_agent` | identity-unavailable test |
| detached cancellation + output labels | private helpers | implemented per source; detached-runner behavioral proof lives in the detached-job slice |

`CANCELLATION_SETTLEMENT_TIMEOUT_MS` is a `tokio::time::timeout` over the
shared dispatch future.

## dispatch-runtime.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `isTerminalAgentLifecycle` | `dispatch_runtime::is_terminal_agent_lifecycle` | registration journal listener |
| `appendParentAgentRecord` | `append_parent_agent_record_for` (delegates to the journal module) | spawn test |
| `dispatchAttachedSessionAgent` | `dispatch_attached_session_agent` | attach/recovery path |
| `startToolDispatch` | `start_tool_dispatch` | spawn/background tests |
| `trackAgentDispatch` | `track_agent_dispatch` | spawn/background tests |
| `runAgentSession` (+ create/admit/lease/complete/fail helpers) | `run_agent_session` and private helpers | spawn completion, wait terminal notification |
| `releaseOwnershipAfterDispatch` | `release_ownership_after_dispatch` | spawn path |
| waiting desktop notification helpers | `remember/close_waiting_desktop_notification`, `notify_waiting_agent` | implemented; notifier unit path |

Completion summaries use the last assistant text; `steering_pending` drains
through `ChildAgentSession::drain_runtime_coordination`.

## spawn-attach-runtime.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `spawnAgentSchema`/`attachSessionAgentSchema` | `spawn_agent_schema`, `attach_session_agent_schema` | tool metadata differential |
| `spawnAgent` (+ validate/prepare/construct/commit/journal/dispatch) | `spawn_agent` and private helpers | spawn success/failure tests |
| `attachSessionAgent` (+ selector/path/match helpers) | `attach_session_agent` and private helpers | attach test |
| `recoverAgents`/`readActiveParentAgentIds` | `recover_agents`, `read_active_parent_agent_ids` | implemented; recovery is exercised indirectly by session-start wiring |
| `resolveConfiguredAgentProfile` | private helper | used by spawn/attach |

## wait-runtime.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `WaitNotificationsWake`, `WaitAgentsWakeWatcher` | `WaitNotificationsWake`, `WaitAgentsWakeWatcher` | wait tests |
| `waitNotificationsRuntime` | `wait_notifications_runtime` | wait tests |
| `consumeNotificationsRuntime` (+ coordination/terminal helpers) | `consume_notifications_runtime` and helpers | wait terminal-notification test |

Rust polls with `tokio::select!` over notifications, the 3s interval, and the
cancellation token. Reconciliation/delivery failures become `Error` wakes;
the source throws from the interval callback (see discrepancy 4).

## runtime.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `MultiAgentRuntimeHandles`/`createMultiAgentRuntimeHandles` | `runtime::MultiAgentRuntimeHandles`, `create_multi_agent_runtime_handles` | all runtime tests |
| `wakeWaitAgentsAfterCoordination` | `wake_wait_agents_after_coordination` | implemented |
| `MultiAgentExtensionOptions` | `MultiAgentExtensionOptions` | registration/request tests |
| background dispatch/command, `jobsCommand` | registration background helpers, `runtime::jobs_command` | bg/jobs tests |
| `createMultiAgentPiRequestHandler` (+ dispatch/select/message summary) | `MultiAgentPiRequestHandler`, `create_multi_agent_pi_request_handler` | request-handler tests |
| `assertNoWaitAgentsParams` | `assert_no_wait_agents_params` | differential wait-parameter cases |
| `waitAgents`/`waitNotifications`/`consumeNotifications` | same names | wait tests |
| `formatAgentStatus` | `format_agent_status` | bg/jobs/wait text |
| `resolveMultiAgentStore` | `resolve_multi_agent_store` | registration |
| `deliverTerminalOutboxForStore` | `deliver_terminal_outbox_for_store` | implemented via ca-detached core module |
| `registerAgentsCoreTools`/`registerAgentViewerTools`/`registerAgentsMailboxTools` | same names | registration/tool metadata differential |
| default `multiAgentExtension` | `multi_agent_extension` | registration differential |
| `resolveSelectedSessionMutationTarget` | `resolve_selected_session_mutation_target` | mutation-target tests + main binding |

## registration.ts

| Source | Rust | Proof |
| --- | --- | --- |
| `createRegistrationContext` | private helper | registration tests |
| session start/shutdown hooks, terminal transition journaling | private hook registration | implemented; metadata path used by spawn test journal |
| `registerBackgroundCommand` (`bg`) | private command registration | bg unavailable test |
| `registerJobsCommand` (`jobs`) | private command registration | jobs test |
| spawn/list/attach/wait/close/steer tools | private tool registrations | tool metadata differential + runtime tests |

## index.ts / src/extensions/multi-agent.ts

Re-export surface only. Rust exports the same items from
`bundled_extensions::agents_core` (`mod.rs` + `runtime.rs`), and
`bundled_extensions::register_agents_extensions` registers
`<first-party:agents-core>`, `<first-party:agent-viewer>`, and
`<first-party:agents-mailbox>` with one shared store/handle set.

## Discrepancies and deviations

1. **Session factory contracts live in `shared.rs`.** The source defines them in
   `spawn-attach-runtime.ts` and imports them from `runtime.ts`; Rust moves the
   trait/factory types to `shared.rs` to avoid a module cycle. Behavior is
   unchanged.
2. **Production factory marker is a boolean.** The source tags the factory
   function with a `Symbol`; Rust stores `production: bool` on
   `ChildAgentSessionFactory`.
3. **`abortAgentHandleSafely`** — the source wraps `abortAgentHandle` in
   try/catch and logs; the Rust store method is infallible (`bool`), so only the
   call remains.
4. **Wait watcher errors** — `reconcileDeadDetachedRuntimes` and terminal
   outbox delivery throw in the source (uncaught inside `setInterval`); Rust
   returns `WaitNotificationsWake::Error { message }` so the wait tool reports
   `Wait failed: ...`. Source bug at the pinned revision (uncaught throw from a
   timer crashes Node); fixed upstream in Pi `9cb4142bb`, which the Rust behavior
   matches (user decision 2026-10-19).
5. **Dispatch cleanup identity** — the source deletes a dispatch only when the
   map still holds the same promise; Rust deletes by agent id (agent ids are
   unique and never re-dispatched).
6. **Desktop notifier shape** — the source accepts a notifier returning a
   handle or a promise; Rust's `DesktopNotifier` trait is synchronous.
7. **`truncateText` UTF-16 boundary** — JS `slice(0, 2000)` can split a
   surrogate pair; Rust drops the astral character at the boundary instead of
   emitting a lone surrogate. Only affects 2000-unit truncation boundaries in
   `messages.last`.
8. **Background ownership release** — handled in the dispatch-completion task
   (matching `promise.finally`), including removal from the handles map.

9. **Async `SelectAgentView`** — the source selector is synchronous; Rust returns
   a `BoxFuture<Option<bool>>` because the interactive TUI owns the mode on a
   `LocalSet` thread and the bridge must await a `UiRequest` response. The
   `agent_viewer` select path awaits it.

Resolved discrepancy: the session-factories gap (the ninth recorded item) is
closed; `CreateAgentSessionOptions` now carries the multi-agent identity, store,
runtime role, session-start event and extension factories.

Port bug fixed (not a source discrepancy): `JournalWriter::from_api` originally
snapshotted `runtime.actions().append_entry` while the extension was still
loading, so every production spawn failed to journal `agent_start` with
"Extension runtime not initialized". The writer now resolves the runtime action
when `appendEntry` is called, matching the source `pi.appendEntry` boundary
(`parent_agent_journal.rs`, end-to-end spawn test).
