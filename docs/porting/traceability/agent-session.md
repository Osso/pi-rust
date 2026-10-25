# agent-session.ts traceability

## Storage-owned terminal drain integration2026-10-25

Bounded [terminal-outbox slice](terminal-outbox-delivery.md#stage2-numericclock-and-caller-slice2026-10-25): synchronous drain now returns CallbackError Result, coordination retains the literal frozen catch/log at3492-3495, and old-only prompt-entry draining is removed (frozen direct callers only1892/3493). A real public-session/source regression uses registered commands, faux turns and persisted outbox attempts; RED native command/start/final1/2/4 versus source0/0/2, then GREEN0/0/2. Source `prompt:3014-3025` handles commands before any prompted-turn/post-prompt coordination. Whole session remains unaccepted; the parent-designated reserved-finalize bool/error sink must close with the stage4 Result listener migration, using the store-local catch/log at multi-agent-store.ts:1202-1211. No uncaught workaround or new semantic deviation is claimed.

Q15 correction2026-10-23: agent_settled/AgentSettledEvent is removed old-only; common-pin packages have no counterpart. This removes the session emission/variant, extension event/registration/dispatch, mode consumers and exports. RPC test waits now observe actual agent_end/command responses. Process RED exposed an extra native JSON line and a frozen RPC wait that could never settle; common-pin RPC differential is now6/6 after context persistence and the source fork/clone error boundary were restored.

Rust: `crates/pi-coding-agent-core/src/core/agent_session.rs` (module `core::agent_session`).

## Q13 working-directory recovery hooks (4b checkpoint)

Frozen :870–871/:2315–2320 maps to optional cloned beforeRun/afterTurn callbacks. :1462 runs afterTurn before the unchanged restriction/prepared/duplicate/previous stop predicates. :2966 idle continue, :3034 lock-reserved prompt and :3251 registered command forward to a distinct replacement session; prompt releases its old turn-start permit first. Known-slash validation follows beforeRun under the permit, not before recovery. AgentSessionError stores a boxed Native/Callback source, retaining arbitrary rejection values and native properties; AgentError conversion preserves its source chain.

Actual runtime/source behavioral RED→GREEN: `continue_recovers_deleted_session_cwd_before_empty_transcript_returns`, see [bounded recovery matrix](agent-session-runtime.md#4b-recovery-checkpoint-2026-10-23). Composite26 current nonignored tests cover idle/streaming prompt and continue, registered-command context, reentrant same-session callback, raw/native/DOMException carriers, prepared relocation priority, real provider-turn activation and isolated process-cwd notice. Public afterTurn failure projects at frozen Agent.runWithLifecycle's catch into an error AssistantMessage; beforeRun failures remain rejected values. No restriction predicate changes or whole-session acceptance.

## Codex restriction context persistence (2026-10-23)

Frozen agent-session.ts:2792–2800,3083 records the original pre-transform/pre-expansion input after extension interception, before queuing/auth. `record_codex_restriction_user_request` writes `{requestId,userRequest}` using the shared UUID authority. Extension-origin, standalone and child/identity-required sessions skip it; empty agent IDs retain JS falsey behavior. `prepare_prompt_input` propagates append failure instead of suppressing it.

Proof: `codex_restriction_context` covers exact raw UTF-16 text, repeated-request UUID refresh, failed-auth ordering, transformed/handled input and six eligibility cases. Full actual-frozen `differential_rpc_mode` GREEN6/6 proves stored entry order and all existing response/event assertions. That checkpoint only persisted context. The following [guarded execution slice](provider-restriction-recovery.md) now wires advisory/validation/one-attempt execution and identity/cancellation guards, with eight bounded actual-frozen lifecycle scenarios GREEN; concurrent-state proof remains open.

## Q13 inherited session test failures (2026-10-24)

The parent serial run (68/71) exposed stale assertions, not production regressions. Frozen :2785–2800,3083 persists a `codex_restriction_context` entry before each supervised human prompt. `prompt_persists_user_and_assistant_messages` now checks that entry's exact data fields, the three persisted messages and their parent chain. Both navigation tests select the actual user entry and assert the restriction-context parent remains the leaf/summary attachment point (:6129–6132,6154–6158; session-manager.ts:2101–2112). They no longer mistake the first custom entry for a user or expect an empty root. Frozen agent-loop.ts:308 continues plain assistant replies until `end_turn`; the original navigation scripts exhausted their queue (and the second prompt's response) inside the first turn. Explicit text-plus-`end_turn` scripts now produce a checked four-entry sequence per turn: restriction context, user, assistant, tool result.

Parallel failures/hangs came from scripted sessions resolving the process-global API registry on each call while other fixtures reset it through refresh/reload or overwrite identical API names. The test-only injected `agent.streamFn` now owns its Faux provider. `scripted_sessions_keep_their_responses_across_other_registry_refreshes` reproduces two same-API sessions plus a registry reset and asserts distinct answers and consumed response queues concurrently. The corrected pre-fix RED returned no second answer because the first registry's refresh replaced the second session's transport. The former partial IDLE mutex is removed; no serialization or production behavior changes are used. At `9f372aac`, the full default-parallel `cargo test -j4 -p pi-coding-agent --test agent_session -- --nocapture` passes72/72 in45.86s, including both previously hanging tests and every thinking/faux case. Workspace all-target JSON check:0 errors/0 warnings; warning-mode target clippy:0 changed-line findings/511 unrelated diagnostics; fmt PASS; inventory686 files/186679 lines PASS, unchanged statuses. Changed Rust readability was reviewed manually (metric binary unavailable). Logs and command/revision proof: `/tmp/pi-rust-repin-lossless-interactive/agent-session-successor-*`. Expected uncaught-persistence panic and activity-publication-error fixture stderr remain visible and are not suppressed. No whole `agent-session.ts` parity claim.

## Restriction reference-identity dependency

The [Agent-owned identity authority](agent.md#frozen-reference-identity) now distinguishes equal-valued replacements from preserved prefix objects. Direct custom/bash appends and retry/overflow prefix slices use the source-preserving native operations. The subsequent execution integration captures the assistant's scoped message_end token alongside its owned value and validates the response against the same Agent-owned snapshot authority; eight actual-source lifecycle scenarios are GREEN.

Session-entry identity dependency2026-10-23: SessionManager owns a token per stored message entry; successful message-end persistence binds the current emitted token, and direct bash persistence binds append's returned token. SDK restoration, runtime setup, compaction, summary editing and tree navigation rebuild through the identity-bearing context projection and restore those tokens atomically. Frozen session-manager.ts:447–469 returns stored `entry.message` directly but constructs custom/branch/compaction objects; :542–560 preserves fitting kept messages and clones only truncated ones. Native uses one projection with origin IDs, not equality or a second context algorithm. `agent_identity_paths`2 GREEN includes actual-frozen context reference observations and real bash persistence; scheduler still pending.

## Thinking timeout dispatch

Partial checkpoint2026-10-23: frozen agent-session.ts:1707–1744,1939–1951,2041–2090,2659–2734,4083,5756–5778 map to native watchdog/ThinkingTimeoutDispatch ownership, one recovery, retry reporting and cancellation. Native `thinking_` focused tests9 GREEN include four new lifecycle tests plus existing main/child/tool/observer proof. Duration is binary64 with Node timeout normalization; default/error text is20 minutes. Each prompt/continuation owns one dispatch across post-run work; manual compaction uses the same continuation entry point.

Actual frozen RPC recovery is now GREEN under the explicit [AGENTS.md](../../../AGENTS.md):19/39 approvals: the source oracle reuses `oracle/assistant-snapshots.ts` to copy actual events at push (no field replacement), and only this async-factory timeout case compares input+cacheRead totals while retaining exact output/cacheWrite/cost/totalTokens and every other value/type/presence. RPC object order follows the pre-existing semantic JSON contract in [rpc-jsonl](../../specs/rpc-jsonl.md); ordinary differential cases are unchanged. Frozen faux.ts465–468's async factory continuation warms the cache before recovery while native scheduling can warm it later. Proof: q15-thinking-timeout-rpc-approved-deviation-green-4.log. See [spec](../../specs/thinking-timeout-recovery.md).

Persisted fallback ordering is now a separate approved deviation2026-10-23, [AGENTS.md](../../../AGENTS.md):40: native-created AssistantMessages use the fixed native order; values/presence stay exact. The source aborted record has `usage,timestamp,stopReason,errorMessage`, native `usage,stopReason,timestamp,errorMessage`. `thinking_timeout_recovery_matches_frozen_rpc` now compares both actual persisted aborted records semantically and then parses/serializes/writes the actual TS-written record byte-exactly. q15-thinking-timeout-rpc-persistence-approved-green.log GREEN. Raw TS-written JSONL order remains preserved; no provenance carrier or persistence fallback was added.

The following [literal idle module/scheduler slice](idle-compaction.md) now implements `_scheduleIdleCompaction` at finalization, session-start and reload, with disposal cancellation; helper96 and scheduler4 tests GREEN. Restriction cancellation/attempt cleanup is now wired by the execution-state slice, whose concurrency/settlement proof remains open. No no-op hooks were added. Follow-up native proof now covers lock-held cancellation through a blocking input hook, one queued agent_end correction delivered before recovery, and one recovery allowance shared with a real extension-driven compact-and-continue path. Composite current proof: thinking_timeout six GREEN in q15-thinking-lifecycle-green-3.log, post-compaction case GREEN after preventing test-clock acceleration through its long streamed reply in q15-thinking-post-compaction-green.log. Production code unchanged.

## Q13 reload context lifetime (2026-10-24)

Frozen reload :5617–5635 and _buildRuntime :5551–5611 do not invalidate the outgoing runner; only dispose :2271 does. The native extra reload invalidation is deleted, not moved. Actual print extension-command process REDs cover pending/failed reload and old-context isIdle after successful reload; see [print-mode proof and source discrepancy](print-mode.md#reload-callbacks-and-old-context-lifetime). This does not change dispose invalidation.

## Q13 display queue projection

Frozen preview type/storage/getters and the interactive visibility consumer are mapped in [coordination visibility](coordination-visibility.md). At `15f0eca7`, the real-stream snapshot/provenance regression passes; full `agent_session` result is 52/53 with only the previously known `runtime_mailbox_delivery_prompts_the_session_and_removes_the_accepted_row` RED (`delivered mailbox prompt`). This is bounded queue proof, not whole-module parity or a mailbox fix.

## Q18 native error/path consumers

Provider human errors, auto-retry start/end error fields, compaction errors and required-auth failures stay native. AgentSession config/inner cwd, agent_dir, control_db_path, session-file accessors and export JSONL path/header values use JsString. Runtime persistence identity comparisons remain raw; export bytes encode only at the actual FS call. `lossless_agent_session_paths` contains a TS-created raw-session-path fixture through AgentSession and MultiAgentStore persistence identity; coding execution remains blocked by concurrent caller/helper migrations. No parent completion claim. Tree targets, leaf/cache IDs, labels, custom-entry and label callbacks remain native; Cow-backed entry projections are owned before crossing guards. Missing tree/materialization entry errors interpolate raw IDs. Materialization checks raw checkpoint provider/api/format/value fields, including the frozen array-value requirement, without decoding unrelated JSON. The same blocked target asserts raw missing-ID error units and reopen identity. Latest compile diagnostic `agent-session-path-compile-3.jsonl` reduced AgentSession primaries from 66 to 49; this is not runtime proof.

## Extension compaction callbacks (common pin, 2026-10-24)

[Contract](../../specs/extension-compact-callbacks.md). Frozen5432-5438 maps to the context `compact` binding and `complete_extension_compaction`: forward actual result, catch completion rejection into `onError`, normalize non-Error with String/error construction, and let error-callback failure reach the existing uncaught boundary. CompactOptions callbacks return existing CallbackError results; onError receives the branded error, not a message-only projection. No other AgentSession behavior changes.

Actual first RED `compact-callback-success-red.log` proves the prior TODO dropped completion. After callback API representation, `compact-callback-histories-red-2.log` runs all3source/native histories and shows only native producer events instead of completion/error/uncaught events. The intermediate API RED and one missing-import build probe are not runtime mismatch evidence. Native proof uses real SDK/extension context and a real compaction override hook, with an explicit private child for escape; source invokes actual `_bindExtensionCore` on the actual prototype with only its compaction producer controlled. TypedError/code and rawD800/DFFF fields are compared. Final GREEN/checkpoint artifacts pending in `/tmp/pi-rust-repin-core-tools/ledger.md`.

## Exported symbols

| TS | Rust | Tests |
| --- | --- | --- |
| `AgentSession` | `AgentSession` | `tests/agent_session.rs` (42), `tests/differential_agent_session.rs` |
| `AgentSessionConfig` | `AgentSessionConfig` | same |
| `AgentSessionEvent` | `AgentSessionEvent` + `AgentMessageEvent` | event assertions + differential |
| `AgentSessionEventListener` | `AgentSessionEventListener` | — |
| `PromptOptions` | `PromptOptions` | prompt/steer tests |
| `ModelCycleResult` | `ModelCycleResult` | cycle tests, differential |
| `SessionStats` / `ContextUsage` | same | stats/context tests, differential |
| `SupervisorDecisionRequester` | same | LLM reviewer path |
| `getAssistantMessageText` | `get_assistant_message_text` | duplicate/stat paths |
| `shouldContinueInterruptedSession` | `should_continue_interrupted_session` | unit helper |
| `isPermissionPromptProtocolTool` | `is_permission_prompt_protocol_tool` | discovery tests |
| `DETACHED_TOOL_CALL_COMPLETION_CUSTOM_TYPE`, `buildDetachedToolCallCompletionEntry` | `DETACHED_TOOL_CALL_COMPLETION_CUSTOM_TYPE`, `build_detached_tool_call_completion_entry` | `tests/agent_session_runtime_mailbox.rs` (detached completion delivery) |
| `bindAgentSessionMutationTargetResolver` | `bind_agent_session_mutation_target_resolver` | `tests/agent_session_mutation_target.rs` |
| `detachRunningTool` / `detachBashTool` / `hasDetachableTool` / `hasDetachableBashTool` | `detach_running_tool` / `detach_bash_tool` / `has_detachable_tool` / `has_detachable_bash_tool` | `tests/agent_session.rs::detach_running_tool_and_detach_bash_tool_delegate_to_the_tool_detach_registry` |
| `getToolDetachRegistry` context action | `get_tool_detach_registry` in `build_extension_context_actions` | same test reads the registry through `ExtensionContext::tool_detach_registry` |

## Behavior coverage

- Constructor: role/capability validation, tool registry, system prompt rebuild, hooks,
  extension runner construction, control-db path installation, permission rule store,
  supervisor requester default, detached-job lifecycle binding.
- Base tools: `createAllToolDefinitions` rebuilt on every `_buildRuntime` from current
  settings (read auto-resize, bash command prefix/shell path/detach registry/background
  jobs); `baseToolsOverride` path appends `end_turn`; active names are deduplicated in
  insertion order. The old BashSessionEnvironment/provider consumer and its old-only
  test were removed mechanically under the [current bash re-pin](bash.md); frozen
  has no session PI_* injection/stripping contract.
- Events: `_handleAgentEvent`, persistence on `message_end` (a failed `appendMessage`/custom/bash append is an uncaught throw, matching the source's handler with no try/catch; proof `tests/agent_session.rs::persist_failures_are_uncaught_like_the_typescript_event_handler`), queue removal,
  duplicate-turn guard, `agent_end.willRetry`, extension event emission,
  runtime message markers (`AgentSessionEvent::Message`).
- `dispose`: aborts retry/compaction/summary/branch/bash and the agent, clears the
  thinking-phase deadline, invalidates the outgoing extension runner with the exact
  stale-ctx message, stops runtime-mailbox polling/wake, disconnects the agent
  subscription, clears listeners, then runs `cleanupSessionResources(sessionId)`.
  Cleanup failures surface as `AggregateError` (throw_uncaught, since `dispose` is
  infallible in Rust). Proof:
  `agent_session_runtime.rs::new_session_invalidates_the_outgoing_extension_context`,
  `new_session_cleans_up_the_outgoing_session_resources`.
- Prompting: `prompt`, `_promptTurn`, input hook, skill/template expansion, streaming
  queue errors, extension commands, `steer`, `followUp`, `continue`, `interrupt`,
  `abort`, `sendCustomMessage`, `sendUserMessage`, `clearQueue`, reservations.
- Models/thinking: `setModel`, `cycleModel`, scoped models, `setThinkingLevel`
  clamping, `cycleThinkingLevel`, queue modes, extension `model_select` /
  `thinking_level_select`, control-db model/thinking persistence. Control-db
  write failures propagate (`set_model`, `cycle_model`, `set_thinking_level`,
  `cycle_thinking_level`, quota fallback) as the source throws; the approval
  reviewer cannot reject through its `Option`-returning hook, so a goal-read
  error blocks the tool call (fail closed, recorded in `session-manager.md`).
- Retry: `_isRetryableError`, quota fallback, `_prepareRetry`, `auto_retry_*` events,
  `notifyProviderRetry`.
- Compaction: `compact`, `_checkCompaction`, `_runAutoCompaction`, `appendCompaction`,
  context estimate, materialization, abort hooks, compaction extension overrides.
- Bash: `executeBash` over `bash_executor` (shell prefix, streaming
  caller onChunk callbacks and abort tokens), `recordBashResult`, pending flush,
  `abortBash`. `detachRunningTool`/`detachBashTool` and
  `hasDetachableTool`/`hasDetachableBashTool` delegate to the session's
  `ToolDetachRegistry` (`detach_running`/`has_running`); detaching does not
  unregister the handle, matching the source where the tool's subscription
  unregisters when its execution ends.
- Permissions: tool gates, auto-approve reviewers, approval orchestrator, MCP
  permission-prompt protocol tool (discovery/config, rule-store skip, updated input
  write-back), human UI reviewer (including the `Allow always` background
  `permission-config` spawn), supervisor LLM reviewer with read-only
  `approvalKind` auto-approval. Reviewer, UI-dialog, and Supervisor request
  failures propagate through `ApprovalReviewer` and become error tool results via
  the agent loop's `prepareToolCall` catch, matching the source instead of
  allowing or blocking the call. Proof:
  `human_allow_always_spawns_the_permission_config_agent`,
  `llm_reviewer_auto_approves_read_only_builtin_tools`,
  `llm_reviewer_does_not_auto_approve_extension_tools_reusing_a_read_only_name`,
  `human_approval_ui_errors_abort_the_tool_call`,
  `supervisor_requester_errors_abort_the_tool_call`.
- Session: names, compaction summary edits, `navigateTree` (no-summary and
  summarization paths with `session_before_tree`/`session_tree` and branch-summary
  attachment), fork selector, stats, context usage, `exportToJsonl`, `exportToHtml`,
  `getLastAssistantText`, `createReplacedSessionContext`.
- Frozen `agent-session.ts:3598-3605` releases the mailbox drain immediately
  before `_promptTurn`. Native passes ownership of the drain guard through
  claimed delivery and drops it in `prompt_mailbox_messages` before `prompt_turn`.
  Acceptance remains in prompt preparation; receipt persistence, canonical-row
  deletion, and finally-style unaccepted-claim release are unchanged. Proof:
  `agent_session_runtime_mailbox::idle_mailbox_prompt_completes_tool_result_and_next_assistant_turn`
  runs idle delivery -> faux read call -> successful tool result -> next assistant,
  with inbound coordination enabled and a five-second Tokio timeout. Retaining
  the guard times out after the successful read result (RED and explicit mutant).
- Runtime coordination: `drainRuntimeCoordination`, mailbox delivery at prompt /
  post-agent-run / after-tool-result checkpoints (steer vs prompt), extension
  `runtime_mailbox` interception, detached tool-call completion entries, shared-channel
  formatting/cursor delivery, listener registration on first drain.
- Multi-agent: activity publishing for `agent_start`, `tool_execution_start/end`, and
  `agent_end` with exact runtime ownership; relocation continuation via
  `AgentEndSessionContinuation::CwdRelocation` and `activateToolResultRelocation`.
  Publication failures throw out of `_handleAgentEvent` (frozen agent-session.ts:1650,
  1766-1774); the Agent reports listener errors and continues the run (frozen agent.ts:615-620),
  so the prompt resolves and no activity is recorded
  (`activity_publication_errors_are_reported_without_aborting_the_run`; the old-pin
  abort assertion was replaced at re-pin).
- Re-pin gap: `runtime_mailbox_delivery_prompts_the_session_and_removes_the_accepted_row`
  is RED. Prompt-mode delivery is gated on `has_configured_auth` (frozen agent-session.ts:3573),
  and the old-only `model_runtime.rs` (model-runtime.ts is absent at the pin; coverage
  "Removed by re-pin") lets a deferred `spawn_refresh` claim a newer availability seq,
  so the awaited refresh's snapshot is discarded. Resolved by the model-registry re-port.
- Thinking-phase deadline: the 15-minute timer is armed on `agent_start` and after the
  last tool result, cleared on tool start/`agent_end`/abort/dispose, restarted by idle
  steering, skipped for observer runtimes and non-positive timeouts, and the recorded
  timeout error replaces the aborted run error. Proof:
  `thinking_phase_deadline_caps_main_session_thinking_but_not_tools`,
  `thinking_phase_deadline_reports_the_child_subject`,
  `thinking_phase_deadline_skips_observer_sessions`.
- Runtime mailbox timers and steering: the 30s polling timer, 60s listener heartbeat,
  SIGUSR2 signal wake with the keepalive guard, dynamic metadata control-db
  resolution, steering delivery acknowledgement, and runtime-mailbox steering turn
  finalization (including deferred terminal results for active descendants).
  Proof: `agent_session_runtime_mailbox` (5), `agent_session_signal_wake` (1).
- Background compaction: speculative cache generated during tool turns at 70% of the
  context window, reused by the next threshold compaction, invalidated on session/model/
  prompt/settings drift. The cache snapshots branch entries and the extension runner
  and honors the `compaction` hook before the local summarizer.
- Compaction extension boundary: `session_before_compact` preflight and the
  `compaction` provider event for manual and automatic compaction, cancel handling,
  extension-provided results when built-in compaction is disabled, and `fromHook` on
  every compaction entry. Proof: `agent_session_compaction_extensions` (9).
- Extensions: runner construction, `bind_core` actions/context/provider actions,
  `bind_extensions`, tool gates/reviewers/tool_call/tool_result hooks,
  `before_agent_start`, `input`, resources discovery, internal `model` command,
  `bind_session_runtime_relocation` + `AgentSessionRuntime::bind_session_runtime_actions`.
- Detached jobs: `bind_detached_job_lifecycle` provider from `MultiAgentStore`,
  terminal-outbox projection delivery on prompt.

## Definition carriers

- `AgentSessionConfig.base_tool_definitions` / `custom_tool_definitions` carry extension-style tool definitions
  that the session wraps with its live `ExtensionRunner` context (source info `<builtin:name>` / `<sdk:name>`).
  They preserve prompt snippets/guidelines and let coordination tools run with the runner context; `base_tools`
  remains the AgentTool seam used by tests.

## Gaps (explicit)

- None.

## Deviations

- `set_sandbox_profile` reads the status color through `try_current_theme()` and
  returns `Theme not initialized. Call initTheme() first.` as a session error;
  the source's `ctx.ui.theme` throws and the caller reports it.

## Discrepancies

- `AgentSession::compact` returns `core::compaction::CompactionResult` instead of
  `unknown`; typed API excludes the JS loose shape (approved typed-core rule).
- Runtime coordination `hasConfiguredAuth` uses the model-runtime snapshot, matching
  the source; a runtime created with `refreshOnCreate: false` must refresh before
  mailbox prompt delivery.
- The `Allow always` spawn failure path logs `Failed to spawn allow-always agent:`
  plus the tool error message; the source's `console.error` also prints the JS
  error stack.
- Differential `differential_agent_session` compares provider usage
  (`input`/`output`/`cacheRead`/`cacheWrite`/`totalTokens`), context usage
  `tokens`/`percent`, events, messages, stats, fork selector, navigation, and
  session/export JSONL byte-for-byte with no numeric normalization. Both sides run
  one active `end_turn` tool and a fixed custom system prompt
  (`"Differential system prompt"`). The oracle's stream adapter removes the
  TS-only `AgentTool` fields `label` and `executionMode` before handing the
  context to the faux provider, matching the provider-facing `Tool` shape Rust
  sends; real provider adapters drop those fields too.
- `getUserMessagesForForking` returns typed `UserMessageForForking` objects matching
  the TS `{entryId,text}` wire shape.

## v15 session names (approved 2026-10-20)

Name methods follow `c248b0067` ([reference lock](../source-lock.json)).
`set_session_name`/`clear_session_name` call the shared manager and propagate persistence errors.
Both publish `SessionInfoChanged` to subscribers and extensions; neither appends JSONL.
Proof: `agent_session::session_name_round_trips_through_session_metadata_state`,
`rpc_mode_session_name` (real RPC process; metadata value and no session-info entry),
`session_autoname` (explicit-clear gating and exact extension set/clear event ordering). The broad pinned AgentSession/RPC differentials
retain name commands/events but capture JSONL before renaming; v15 persistence has separate proof.

## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).

`export_to_jsonl` also uses shared stringify for header and linearized entries, matching pinned `agent-session.ts:5999,6005`. `jsonl_export_preserves_javascript_value_bytes` writes real JSONL with numeric keys, float-backed integers and an unsafe integer; only serialized bytes change.

## Q18 native consumer follow-up
Control DB, model/runtime and session-manager errors propagate message authorities rather than Display encoding. Native skill/template names and paths flow through slash commands and skill invocation. Skill expansion now follows frozen `_expandSkillCommand`: parse/read failure emits `skill_expansion` and preserves the original command; the old Rust parser-to-original-file fallback was an implementation discrepancy, not a frozen-source behavior. `invalid_skill_frontmatter_emits_native_path_error_and_keeps_original_command` was added before replacing the fallback; `skill-expansion-red.jsonl` remains library-compiler-blocked. Runtime acceptance not claimed.

## Q18 remaining native adapters (2026-10-22)

Tool prompt snippets/guidelines and stored maps, discovered extension resources/
source labels/baseDir, Bash shell options, and settings-backed theme selection
retain native text. Permission tool names remain approved opaque scalar metadata;
unpaired settings values reject explicitly at narrowing. SDK default provider/
model IDs use the same explicit rejection. No Display round trip.

Supervisor project resolution and goal/request construction now occur inside
SupervisorApprovalCall. Frozen agent-session.ts:1389 and approval-reviewer.ts:12
propagate thrown request-callback failures; there is no catch or human/deny
fallback for thrown errors. The old default project ID is removed. An isolated
process test constructs a real session with invalid raw-key project config and
asserts native error plus no requester invocation. Snippet/resource-label fixtures
check exact raw units. Written but unexecuted while coding compilation is blocked.

Remaining diagnostic dependencies at replacement-adapters-1: parent native Bash
executor/controlDB/mailbox APIs, UI theme-name result, and209 claim ID; parent
already has several of these awaiting the lane's next clean refresh.

Q15 correction2026-10-23: bash_execution_update/BashExecutionUpdateEvent and ExecuteBashOptions.id are removed old-only. Frozen executeBash5807–5810 accepts only excludeFromContext/operations and passes the caller onChunk directly at5825. Real callback streaming/commit assertions remain in execute_bash_records_streams_and_commits; raw RPC request IDs remain proven on actual bash responses and the full source/native bash-response process fixture, not an invented chunk event. UserBashEvent.cwd stays native through extension dispatch.

## Q5 executable-availability consumer (2026-10-23)

Frozen `agent-session.ts:5551–5579` builds default base definitions through `createToolDefinitionsForAvailableExecutables`; the override branch is unchanged. Native `rebuild_base_tool_definitions` uses the same helper. Both it and `build_runtime` return Result, propagated through AgentSession::new and reload; a deleted process cwd remains a catchable build failure, not a process crash. Failed availability lookup does not replace the prior definition/tool registry.

`session_executable_availability` runs isolated child processes with controlled PATH and private HOME/state. Real AgentSession registry names match the frozen index's filtered definitions. A second process case removes the current directory and compares the shared executable helper plus native reload rejection with the actual frozen `_buildRuntime` method's uv_cwd throw and retained-registry state. The existing shared node_fs formatter already matches Node's contextual deleted-cwd message; no formatter or existing generator/core expectations changed. Native AgentSessionError retains its established message-based boundary.

RED log: `/tmp/pi-rust-repin-core-tools/session-availability-red.log`: registry contained 19 rather than frozen 16 names; reload incorrectly returned success after cwd deletion. GREEN/check recorded by the lane checkpoint. Index and utility details: [tools index](coding-agent-tools-index.md), [executable](executable.md).

## Q15 current-state statistics correction

Frozen `agent-session.ts:6233–6280` iterates state.messages and sums assistant usage only. Native get_session_stats formerly folded all persisted entries, retaining7 historical messages after navigation selected an empty root. navigate_tree already rebuilds Agent messages correctly (frozen:6177–6179); navigation and cwd recovery remain untouched. Private add_session_message now accepts current Agent messages; caller audit found no other helper consumer. [Contract](../../specs/session-statistics.md).

Behavioral RED: `q15b-differential_agent_session-current-4.log`, all pre-navigation/events/export assertions match, post-navigation native7 vs frozen0. Source oracle now uses actual ModelRegistry/AuthStorage and Agent, retains tool metadata, and applies the approved2026-10-22 immutable-event snapshots at source stream.push through shared assistant-snapshots.ts (frozen faux.ts:386 pushes before :388 mutates arguments). No argument assertions weakened.

2026-10-25 UUID checkpoint follow-up: approval dialog raw-string rejection exposed three HandlerError→AgentError message-only conversions (tool gates, auto-approved review, approval orchestration). All now use the shared Error.message/String(error) policy and retain the original callback; `agent_session::human_approval_ui_errors_abort_the_tool_call` is the production-boundary RED/GREEN regression.

## Re-pin model/thinking audit (2026-10-25)

Frozen :4120–4326 audited against native model selection, scope cycling, thinking and queue-mode setters. Explicit scopes filter only configured auth, not catalog membership. Scope coverage and current position compare provider/id via `modelsAreEqual`, not whole model metadata. Cycle now uses the same selection path as direct set: child model publication precedes mutation, settings defaults stay unchanged for child agents, session model/thinking persistence and `model_select` source remain exact. Actual frozen `cycle_model_matches_frozen_for_unregistered_scope_and_changed_metadata` covers forward/backward cycling with an authenticated saved model absent from the current catalog and with changed display metadata. RED `/tmp/repin-agent-session-cycle-red.log` returned two nulls instead of the frozen selections. Existing thinking tests cover clamping, persistence and child publication.

Whole-module re-pin remains partial: prompting/queues/hooks, compaction/retry, branching/tree, bash/export and event audit still open; runtime helper wiring awaits the activity-detail lane. Runtime scheduling and adversarial input gaps follow the repository parity bar.

## Re-pin quota fallback (2026-10-25)

Frozen :1514,5650–5691 now uses the existing shared `codex_quota_fallback` map/predicate, a provider-visited set cleared on each user message, ordered authenticated fallback lookup across all three accounts, source-preserving assistant removal, model persistence and the `model_select` fallback event. Deleted the old two-provider pair helper, duplicate quota matcher and single-attempt boolean. Actual-frozen `quota_fallback_visits_all_three_accounts_once_like_frozen` stamps each error through a distinct account-owned Faux core, records requested providers and compares final model; both sides must visit codex → gc → team exactly once. RED `/tmp/repin-agent-session-quota-red6.log` proves native stopped at gc while frozen reached team. Earlier fixture probes are not proof: a shared Faux core overwrote error provider metadata with the initial account.

## Re-pin tool activity detail wiring (2026-10-25)

After merging parent `4eff32510`, frozen :1745–1753 now calls the shared `agent_activity_detail::summarize_tool_arguments` on tool start, stores the exact UTF-16 detail with the active tool, and publishes it through `AgentCurrentActivity::Tool`. Extended the real provider/tool/store lifecycle test `publishes_multi_agent_activity_across_agent_and_tool_events` to assert `query: " inspect\n  files "` publishes `detail: "inspect files"`; RED `/tmp/repin-agent-session-activity-red.log` observed `None`. The shared helper's frozen differential remains owned by the runtime-services lane. No runtime-module or cross-lane semantic edits.

## Re-pin idle steering return boundary (2026-10-25)

Frozen :3360–3383 starts idle continuation without awaiting provider completion; native previously awaited the whole turn. `steer` now spawns the continuation with the same logged-error boundary. Actual-frozen `idle_steer_returns_before_the_provider_turn_finishes` uses a gated provider response and an existing transcript, proves the provider started, and asserts steering settled before releasing that response. RED `/tmp/repin-agent-session-steer-red3.log`: native false versus frozen true. No timeout or assertion was weakened; the response gate is released and the agent settles before test exit.

## Re-pin compaction preflight result (2026-10-25)

Frozen extensions/types.ts:1300–1302 exposes only `cancel` on `SessionBeforeCompactResult`; frozen agent-session.ts:4375–4389 and auto-compaction ignore any legacy `compaction` property. Removed that old field from the native result type and stopped applying preflight summary overrides. Only the `compaction` event supplies summaries. Mechanical fallout: `pi-coding-agent-tools/core/extensions/types.rs` and extension cancellation fixture. Parent explicitly approved field deletion in the same commit. `manual_compaction_preflight_only_cancels_and_forwards_custom_instructions` sends legacy serialized data through the real native handler and compares an actual frozen manual-compaction invocation with an injected preflight result: both must reject disabled built-in compaction without a compaction-provider result or saved compaction. RED `/tmp/repin-agent-session-preflight-red5.log` observed native legacy summary versus frozen disabled error. The fixture has seven messages, exceeding frozen's three-message protected suffix. An earlier under-seeded fixture never reached preflight; a user-ended native legacy-success fixture resumed unexpectedly and was cancelled with its entire test/oracle process groups, verified exited. Those probes are not parity evidence.

## Re-pin manual compaction turn-start lock (2026-10-25)

Frozen :2090–2110/4541 releases turn-start ownership before awaiting the resumed provider turn, and flushes pending bash messages in its finalizer. Native previously retained the guard through that entire resumed turn. `compact_inner` now transfers its existing guard to the continuation helper, which releases it before awaiting the continuation and always flushes pending bash. No second lock or coordinator was added. `resumed_manual_compaction_releases_turn_start_for_streaming_input` gates the real resumed provider, submits a streaming follow-up prompt, and asserts acceptance before the response gate opens; its oracle invokes actual frozen compaction with an injected compaction result and the same provider gate. Native RED `/tmp/repin-agent-session-compaction-lock-red.log` timed out at input acceptance.

## Re-pin branch-summary authentication (2026-10-25)

Frozen :6097 uses `_getRequiredRequestAuth`, not best-effort compaction auth. Tree summarization now rejects missing credentials before invoking the provider, preserving the full login guidance. `navigate_tree_summary_requires_auth_before_calling_provider_like_frozen` compares an actual frozen missing-provider request against native with a queued summary that would otherwise succeed. RED `/tmp/repin-agent-session-tree-auth-red.log` observed native success versus frozen auth error. The oracle receives native's package directory through `PI_PACKAGE_DIR` so both complete error strings name the same installation docs; no message content or fields are removed.

## Re-pin empty tree summary and label (2026-10-25)

Frozen :6156/6168/6182 tests summary and label truthiness before persistence. Native now skips empty extension summaries and labels instead of creating an empty branch-summary/label entry. Real native extension/store test `navigate_tree_empty_extension_summary_and_label_match_frozen` compares actual frozen navigation with an injected empty summary/label: no saved summary, no labels, root leaf, absent result summary entry. RED `/tmp/repin-agent-session-tree-empty-red2.log` observed one summary and label plus a non-root leaf.

## Re-pin JSONL export filesystem boundary (2026-10-25)

Frozen :6353–6390 retains Node filesystem failures. JSONL export now checks whether the parent exists before mkdir, distinguishes file open from write, and retains native message/name/code/errno/syscall/path via the shared Node error carrier instead of wrapping errors in Rust prose. Actual-frozen `jsonl_export_directory_target_preserves_frozen_native_error` compares complete errors for a directory output, an existing-file parent and an intermediate non-directory component; the original file bytes must remain unchanged. RED `/tmp/repin-agent-session-export-red.log` showed EISDIR metadata lost; RED2 showed native EEXIST mkdir versus frozen ENOTDIR open when the parent file already exists. OS-fault shapes remain the repository's one-line gap, not additional recovery behavior.
