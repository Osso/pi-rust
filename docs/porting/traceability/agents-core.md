# agents-core traceability

Successor checkpoint: parent `5f1fa82a8` merged; locked inventory remains 686 rows / 186,679 lines. Background construction-error UTF-16 GREEN: `/tmp/agents-core-3-background-green.log`; 55 focused runtime/viewer/deterministic tests pass (`/tmp/agents-core-3-runtime.log`). Removed the source-absent `parent_agent_journal` module, dispatch journal wrappers/constants and four obsolete journal tests; historical journal maps below are retired, not current proof. The generic failed-message constructor remains an implementation helper for frozen emptyMessage/emptyParentRequest/emptyDirectMessage, not an old-source API contract. Removed a duplicate fixture type property exposed by strict no-emit checking; all three existing agents-core oracles pass with source compiler/type mappings and approved ES2024 target (`/tmp/agents-core-3-oracle-typecheck-green.log`). Workspace/all-target check and formatting pass (`/tmp/agents-core-3-cleanup-check.log`). Runtime remains partial until whole-module trace cleanup and acceptance.

Lifecycle observer diagnostic correction: frozen `runtime.ts:2932` lets mirror errors reach the store's observer boundary (`multi-agent-store.ts:1175–1179`); native caught them locally and emitted a different visible prefix. The real background-runner invalidation regression now asserts the exact observer diagnostic, not just the final uncaught line. RED `/tmp/agents-core-3-mirror-red.log` shows `Failed to mirror lifecycle notification message_1: ...` instead of `MultiAgentStore lifecycle notification listener failed: Error: ...`. The callback now returns the original error to the existing store reporter. Native GREEN: `/tmp/agents-core-3-factories-drift-green.log` (five drift cases, real child end-to-end and all 19 factories pass).

Attachment reservation correction: frozen `runtime.ts:1256–1269` propagates dispatch reservation errors after the attachment record is committed. Native `dispatch_attached_agent` converted every error to `None` and returned successful `Attached ...` output. A native SQLite-trigger regression fails only the runtime-owner insert after real attachment admission; it asserts the original error, no factory/dispatch and the retained waiting attachment. RED `/tmp/agents-core-3-attach-reservation-red.log`; Result now propagates through this caller. GREEN pending.

## Runtime re-pin lane — 2026-10-25

Successor factory slice: child and attached factories now forward the parent's model registry, install browser-only restricted read definitions (including the configurable root), augment profile tools with ask_questions and browser read, and exclude change_working_directory. Attached sessions restore archived files through the control DB and leave settings creation to SDK instead of overriding policy. Actual frozen factory-option differential RED: `/tmp/agents-core-2-factory-red.log`; archive/policy behavioral RED: `/tmp/agents-core-2-attached-red.log`. Factory differential, archive and no-policy-override cases pass at `4fc421ff5` (`/tmp/agents-core-2-factory-green2.log`); one legacy tool-order assertion still needed the frozen ask_questions insertion order and is corrected in the next slice. Mechanical API fallout: main.rs and native factory fixtures initialize browser_agent_config_root. No storage internals changed.

| Frozen runtime branch | Native/proof |
| --- | --- |
| Spawn without parent journal dependency or transcript requirement (`runtime.ts:1080–1198`) | Removed old-only journal admission/start/terminal writes and the native-only request-handler journal argument. Actual frozen spawn with no transcript returns dispatched=true/completed and zero custom entries; native RED returned journaling-unconfigured; GREEN checks dispatch, completion, SQLite state and no parent custom entry. `/tmp/agents-core-journal-{red,green2}.log`; all 25 runtime tests pass |
| Current child assistant result/errors | [Whole child-response map](child-response.md), real dispatch RED/GREEN and nine frozen vectors |
| Descendant settlement and direct detached terminal cleanup | [Settlement](descendant-settlement.md), [cancellation](detached-runtime-cancellation.md); real spawn/SQLite/mailbox boundary |
| Child session shutdown scope (`runtime.ts:3181–3187`) | Actual frozen/native hook differential proves child quit and main reload do not invalidate shared dispatches, while main quit does. Native RED incorrectly invalidated shared store on child shutdown; GREEN `/tmp/agents-core-shutdown-scope-green.log` |
| Remaining runtime audit | Recovery still has an old-only parent-journal admission filter; factory browser/read/tool/profile/archive/registry fields, artifact-root override, configurable cancellation timeout and remaining runtime branch proofs remain open. Child-session shutdown/finally is now closed in its own map. These are not completion evidence |

Mechanical fallout: `pi-coding-agent/src/main.rs` drops the retired request-handler argument; native fixtures use the updated signature. No source delta or storage algorithm change.

Successor cancellation/recovery/artifact slice: removed journal admission from recovery; cancellation now honors cancellationSettlementTimeoutMs and rejects failed dispatch settlement. Factories register agentDir per store with weak ownership; terminal publication and wait reconciliation use that root. Behavioral REDs: `/tmp/agents-core-2-{timeout-red2,recovery-red2,artifact-red4}.log` (17ms requested versus old 5s; absent journal blocked resume; separate artifact root was not cleaned). GREEN pending after commit. Artifact test uses a separate root, not the session directory, so the session-root path cannot mask this bug.

Whole-module audit follow-up: wait coordination wakes now return the supplied prompt; claimed messages are returned in runtimeMailboxDelivery, not marked delivered before durable acceptance, and bridge waits release their claims. Claim failures release unaccepted messages; remote-origin shared channel messages are not filtered as local echoes. Path attachment restores archives and reads the control DB name/subagentName, and parentless attachments retain frozen on-request policy. Fresh child factories also validate active tool identity. RED: `/tmp/agents-core-2-{wait-red2,attach-path-red,fresh-id-red}.log`; recovery fixture now registers a real supervisor listener and simulates a dead owner, with the journal gate mutation failing `/tmp/agents-core-2-recovery-fidelity-red.log`. Pre-commit workspace/all-target check passes (`/tmp/agents-core-2-workspace-check.log`). GREEN pending.

Whole-module audit, dispatch/mailbox follow-up: transcript publication failures now enter the child failure path before prompting; terminalization and cancellation-ack errors propagate rather than disappearing. Frozen mailbox error strings include their target; empty target-session strings use source truthiness. Removed old header/metadata parent routing and old detached-terminal dedup scan in favor of the exact frozen detached/suppression predicate. Bridge messages.send does not invoke the model-tool-only callback. Tracked-terminal wakes preserve captured Set order. REDs: `/tmp/agents-core-2-{transcript-red2,handler-callback-red3,mailbox-errors-red,mailbox-empty-red,mirror-red,parent-route-red,wait-order-red2}.log`. Pre-commit workspace check: `/tmp/agents-core-2-audit-check.log`. Parent `de46a2789` merged; fixture fallout uses valid absolute output paths and the persisted archive API to preserve metadata.

Whole-module profile audit: requested unknown profile errors now precede persistence checks and list configured keys in frozen order. Spawn model metadata inherits the parent's model when needed, starts at medium rather than parent effort, and clamps profile effort to model capabilities; attachment metadata remains the unclamped configured profile metadata. Failed-child commit/publication errors now propagate. Actual public frozen/native five-vector RED: `/tmp/agents-core-2-spawn-profile-red.log`; pre-commit workspace check `/tmp/agents-core-2-profile-check.log`. GREEN pending.

Final audit additions: list_agents treats an empty parentId as an absent filter (frozen deterministic RED `/tmp/agents-core-2-list-red.log`). Browser factory proof now executes the actual frozen/native read definition for both child and attached sessions, with allowed and forbidden paths under a root override. The runtime oracle is split into named scenario functions; strict protected no-emit typecheck passes (`/tmp/agents-core-2-oracle-typecheck.log`). Workspace/all-target check passes (`/tmp/agents-core-2-final-check.log`); focused GREEN pending after commit.

Final scoped acceptance found one obsolete end-to-end expectation: real child completion already passed, but the test still demanded removed agent_start/agent_complete parent records. It now asserts their absence, matching actual frozen spawnWithoutJournal. Unreachable dispatch/background rejection paths use the shared uncaught authority after cleanup; background ownership release runs on rejection as well. New-line clippy findings (test err-expect and borrow) are fixed; wait tests moved after production items. Pre-commit workspace/all-target check `/tmp/agents-core-2-cleanup-check.log` passes. The updated [runtime spec](../../specs/agents-core-runtime.md) describes these contracts.

## Module HOME snapshot (2026-10-23)

Frozen `runtime.ts:198` evaluates `join(homedir(), "AgentConfig")` during static imports (`cli.ts:11` → `main.ts:14` → agents-core runtime), before dispatch or the main action catch. Native `runtime::DEFAULT_BROWSER_AGENT_CONFIG_ROOT` stores that one-time native result; the pi executable initializes it before cli_entry and routes failure through shared `throw_native_uncaught`. The accessor returns the stored root, not a fresh HOME probe. `differential_cli_import_home` compares real frozen/native --version failures under forced NSS EIO (exit 1, exact SystemError prefix, no version output), and a child proves later HOME changes do not alter the snapshot. RED: `pi-cli-nss-frozen-red.log`; GREEN: `pi-cli-nss-green.log`.

| Frozen consumer | Status |
|---|---|
| `runtime.ts:589–595` createBrowserReadTools / browserAgentConfigRoot override | Open: native browser factory wiring is unported; stored default is available through runtime::default_browser_agent_config_root |
| `runtime.ts:699–701` production child factory customTools | Open: does not yet install the restricted browser read definition |
| `runtime.ts:757–759` production attached factory customTools | Open: same missing browser tool wiring; this checkpoint does not claim those consumers |

## Drift implementation / proof status

Actual in-crate baseline `agents_core_drift` at `27410344`: five intended behavioral REDs, `agents-core-drift-incrate-red-2.log`; source cases executed through Cargo's mandatory runner.

| Boundary | Implementation | Proof |
|---|---|---|
| Registration API errors | Helpers/top-level registration propagate the original error in source order | GREEN at `4636282b`, `agents-core-drift-incrate-green.log` |
| Completion-time UI lookup | Resolve current UI after dispatch settles; errors reach the existing process uncaught handler | GREEN at `d03f49d3`, `agents-core-ui-fidelity-green.log`; actual frozen runner observer evidence asserted |
| Post-construction session lookup | Fallible lookup occurs after construction and before admission; no error-to-path fallback | GREEN at `4636282b`, `agents-core-drift-incrate-green.log` |
| Accessed profile model / unrelated fields | Raw profile getter; model registry read precedes source-equivalent model-type error; consumers project accessed fields only | Both GREEN at `4636282b`, `agents-core-drift-incrate-green.log` |
| Tracked-agent DB read | Native error wake from notification and terminal-settlement reads | Actual `cargo test -j4 -p pi-coding-agent --lib tracked_read_failure_tests::tracked_agent_read_failure_wakes_with_error_instead_of_disappearing` RED at `5456b9f1` (`wait-read-incrate-red.log`); correction at `4cffa3fe` passes the same Cargo regression (GREEN1, `wait-read-incrate-green.log`), covering notified and terminal-settlement paths |

### Stale-context fixture fidelity

The initial source stub staled only UI while native invalidated the entire runner; that source-only result did not justify ignoring an extra native mirror diagnostic. The oracle now uses the actual frozen `ExtensionRunner.createCommandContext()` and `invalidate()`. `agents-core-ui-fidelity-red.log` (Cargo invocation) proves source also reports a stale lifecycle observer: `runner.ts:678,850` → `runtime.ts:2974,2932` → `multi-agent-store.ts:1177–1179`. Native reporting remains untouched. The regression asserts the oracle's observer diagnostic, then compares the terminal uncaught line exactly; observer formatting/stack compatibility is not claimed. The earlier relaxed-assertion run is not acceptance evidence.

## Source-backed error boundaries

`agents-core-drift-oracle.ts` imports the actual unmodified frozen runtime module and SettingsManager. Protected source-only execution (exit 0; lane log `agents-core-drift-source-2.log`) confirms public `agents.spawn` with a truthy numeric configured model throws `TypeError: modelReference.trim is not a function`, and public registration propagates a late `list_agents` registration exception. `tests/agents_core_drift.rs` records public Rust handler/registration RED intents; unexecuted while the coding library is blocked. Registration failure injection invalidates the real ExtensionRuntime after early tool setup, rather than substituting an API implementation. Spawn fixtures supply the native entry's required parent-journal dependency using the real SessionManager and active runtime guard, so they cannot fail merely on missing journaling before the profile/session boundary. Remaining production corrections require their in-crate behavioral RED; the status table above tracks implementation and proof separately.

Frozen getter (`settings-manager.ts:509–516`) only structured-clones the configured/default profile; it does not reject malformed untouched fields. A future fix must keep that permissiveness and fail only at the accessed model field, matching the frozen error. `public_spawn_preserves_profile_context_when_unrelated_tools_are_malformed` additionally compares the actual source/native child-factory context for `{context:"fresh",tools:7}`; it prevents whole-profile deserialization from dropping a valid accessed field because an unrelated field is malformed. This new guard is unexecuted, including its source case; source code confirms direct context forwarding. Public `agents.spawn` session-getter fixture now uses real SessionManager/SQLite persistence and a child factory that invalidates context after construction. Frozen source throws the getter error and admits zero agents (`agents-core-session-getter-source-3.log`, protected source-only exit 0). The Rust regression asserts both the error and absence of an admitted row, preventing a later stale error from hiding the swallowed commit-time failure. It remains unexecuted. Public `/bg` lifecycle fixture now confirms UI acquisition occurs at completion, not dispatch start: real frozen source completes the agent and sends a second notification while context is active; after UI invalidation it sends no terminal notification and reports the getter error as an unhandled rejection (`agents-core-ui-source.log`, protected source-only exit 0). The Rust RED-intent uses the real registered command, persisted lifecycle, controlled child prompt and isolated subprocess uncaught handler; it checks both valid notifications and failure exit/message. The status table above records Rust execution and pending corrections.

## Foreground wait slice refresh

Frozen runtime.ts:196,2126-2295,3157-3168 maps to shared `latest_model_request_started_at`, model-request-start and agent/session-end hooks, and the captured deadline in native wait_runtime.rs. `wait_slice_uses_latest_model_request_and_does_not_cancel_active_agents` uses a real SQLite active child and paused Tokio timers: timedOut true, child still running, agent_end clears the deadline. Model-request-start/end events now exist in extension types/runner and AgentSession forwarding. [Contract and remaining event-boundary proof](../../specs/agent-wait-slice.md).

## Frozen 4f9e6f8a4 refresh — 2026-10-21

Historical module inventory below is not renewed source correspondence. Frozen `runtime.ts` now owns private `emptyAgent` (3065), `isInFlightLifecycle` (2526), result helpers (391-412), prompt validators (424-455), `listAgents` (1875) and wait validation (993). The deterministic oracle loads an in-memory transpiled copy and exposes those declarations without editing frozen files; viewer now loads `extensions/agent-viewer/src/runtime.ts:85`.

Frozen `runtime.ts:402-412` and agent-viewer result constructors omit `isError`; native `error_tool_result` now does the same. Explicit per-call-site flags remain separate behavior. Refreshed deterministic oracle reproduced the prior `true` mismatch.

Retired differential cases: generic `createFailedMailboxMessage` and `appendParentAgentStart` are absent from frozen non-test source; removed `failed-message` and `journal` cases are not new-pin parity evidence. Native legacy helpers/tests remain outside this bounded oracle repair. Registration still invokes real frozen exported registration functions.

Current proof (2026-10-21 question lane): deterministic cases and full frozen registration differential pass, including 12/12 exact metadata and set_agent_model ordering. Child-aware ask_questions/answer_question are implemented with durable tool-result receipt acceptance; [relay traceability](question-relay-tools.md) tracks remaining branch proof. toHost canonical routing, historical viewer trace and 25-minute wait slice are now ported with focused proof; actual host-relay transport, shared lossless TextContent/details and whole-runtime gaps remain, so metadata equality is not whole-runtime parity.

Runtime/store/lifecycle old-clock failures were expired canonical rows: frozen `multi-agent-store.ts:939-945` persists the store row before runtime enqueue (`runtime.ts:2954-3040`). Fixtures now use current birth times. Child-disabled capability includes frozen `answer_question`; `set_agent_model` was already registered natively and only missing from the old fixture expectation.

Source: `packages/coding-agent/extensions/agents-core/src/*.ts` (pinned by
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/agents_core/`.

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
