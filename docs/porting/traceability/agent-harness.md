# Agent harness traceability

Pinned source: `packages/agent/src/harness/agent-harness.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`. Rust: `crates/pi-agent-core/src/harness/agent_harness.rs`.

The pinned TypeScript module is an explicit scaffold. `create` opens only
record-free sessions, configuration accessors work, and every effectful
operation rejects with `HarnessNotImplemented`, or with `HarnessClosed` after
`close`. The Rust port preserves that observable contract and does not invent
run, queue, compaction, or persistence behavior that the source does not have.

| TypeScript source | Observable contract | Rust item and proof |
|---|---|---|
| `agent-harness.ts:28-55` | Fourteen `TaggedError` classes with `_tag`, `name`, `message`, and declared fields | `tagged_error!` structs (`LaneBusy`, `MissingIdentities`, `NoActiveRun`, `NoActiveOperation`, `NothingToResume`, `InvalidMessage`, `UnknownSkill`, `UnknownTemplate`, `UnknownTarget`, `UnknownQueueItem`, `LaneExists`, `InvalidLane`, `NothingToCompact`, `Closed`) with `tag()`, `Display`, `Error`; `unimplemented_error_exposes_the_operation_name`, differential `harness_unfinished` |
| `agent-harness.ts:57-65` | `HarnessFault` preserves message and arbitrary cause | `HarnessFault { message, cause: Box<dyn Error + Send + Sync> }`; compile-level; unused by the scaffold |
| `agent-harness.ts:67-72` | `HarnessClosed` message and name | `HarnessClosed`; `reports_harness_closed_for_unfinished_operations_after_close` |
| `agent-harness.ts:74-81` | `HarnessNotImplemented` message and `operation` | `HarnessNotImplemented`; `unimplemented_error_exposes_the_operation_name`, differential `harness_create_restore` |
| `agent-harness.ts:84-87` | `OperationError { code; message }` | Reuses `harness::session::types::OperationError`; no duplicate type |
| `agent-harness.ts:89-103` | `RunOutcome`, `CompactionOutcome`, `NavigationOutcome` unions | `RunOutcome`, `CompactionOutcome`, `NavigationOutcome`; unreachable in the scaffold (explicit gap below) |
| `agent-harness.ts:105-132` | Rejected unions and `ResultValue` result aliases | `RunRejected`, `CompactionRejected`, `NavigationRejected`, `ResumeRejected`, `QueueRejected`, `CancelQueuedRejected`, `AbortRejected`, `CreateLaneRejected`; `RunResult`/`CompactionResult`/`NavigationResult`/`QueueResult`/`CancelQueuedResult`/`RecordUsageResult`/`AbortResult`/`ResumeResult`/`CreateLaneResult` |
| `agent-harness.ts:134-138` | `NavigateOptions` | `NavigateOptions` |
| `agent-harness.ts:140-150` | `SuspendedOperation` | `SuspendedOperation`, `SuspensionReason`, `AbortQueues`, `MissingCapabilities` |
| `agent-harness.ts:152-180` | `LaneInfo`, `QueuedItem`, `LaneSnapshot`, `SessionSnapshot` | `LaneInfo`, `LaneOperation`, `LaneOperationStatus`, `QueuedItem`, `LaneQueues`, `PendingWrite`, `LaneSnapshot`, `SessionLaneInfo`, `SessionSnapshot` |
| `agent-harness.ts:182-196` | `ActionInfo` union | `ActionInfo` with `FactName`, `TryFinishRunOutcome`, `FinishOperationOutcome`, `QueueName`, `AssistantStep` |
| `agent-harness.ts:198-209` | `HookName` literals | `HookName` plus `as_str()` |
| `agent-harness.ts:211-235` | `Hooks`/`Events` registries whose `on` throws `HarnessNotImplemented` or `HarnessClosed` | `Hooks::on`, `Events::on` returning `Result<Unsubscribe, HarnessUnavailable>`; `rejects_every_unfinished_public_operation_explicitly`, `reports_harness_closed_for_unfinished_operations_after_close`, differential `harness_unfinished` |
| `agent-harness.ts:237` | `HarnessTool = AgentTool & { replay? }` | `HarnessTool { tool: Arc<dyn AgentTool>, replay: Option<ReplaySafety> }`; `active_tools_default_to_tool_names` |
| `agent-harness.ts:238-241` | `Resources`, `StreamOptions`, `StreamOptionsPatch`, `EntryProjector` aliases | `Resources`, `StreamOptions`, `StreamOptionsPatch`, `EntryProjector`; `keeps_scaffold_safe_configuration_as_defensive_copies` |
| `agent-harness.ts:243-263` | `AgentHarnessOptions` fields, including ignored ones | `AgentHarnessOptions` + `AgentHarnessOptions::new`; focused tests construct every exercised field |
| `agent-harness.ts:265-269` | `WatchHandle<TSnapshot>` | `WatchHandle<TSnapshot>`; never constructed by the scaffold |
| `agent-harness.ts:271-303` | `AgentLane` interface | Marker trait `AgentLane`; method surface gap below |
| `agent-harness.ts:305-320` | Constructor state, `name = "main"`, `session` identity | `AgentHarness::new`, `name()`, `session`; `opens_only_record_free_sessions_before_restore_is_implemented`, `shares_the_durable_session_with_the_original_handle`, differential `harness_create_empty` |
| `agent-harness.ts:322-332` | `create` rejects when any record exists, with `create.restore` | `AgentHarness::create` returning `CreateHarnessError`; focused and differential `harness_create_restore` |
| `agent-harness.ts:334-336` | `unavailable` selects `HarnessClosed` over `HarnessNotImplemented` | `AgentHarness::unavailable`; closed and open rejection tests |
| `agent-harness.ts:338-425` | Every unfinished operation rejects with its exact operation name | `prompt`, `skill`, `prompt_from_template`, `compact`, `navigate_tree`, `resume`, `abort`, `steer`, `follow_up`, `next_run`, `cancel_queued`, `record_usage`, `wait_for_idle`, `run_when_idle`, `peek_action`, `execute_action`, `run_to_completion`, `watch`, `lane`, `create_lane`, `lanes`, `watch_session`; 22 rejection assertions in both the focused and differential tests |
| `agent-harness.ts:427-455` | Model, thinking level, active-tool accessors with defensive copies | `get_model`/`set_model`, `get_thinking_level`/`set_thinking_level`, `get_active_tools`/`set_active_tools`; focused copy test |
| `agent-harness.ts:456-490` | Tool and resource accessors with defensive copies | `get_tools`/`set_tools`, `get_resources`/`set_resources`; focused copy test, differential `harness_config_roundtrip` |
| `agent-harness.ts:491-528` | Stream options, retry policy, compaction settings, steering/follow-up mode accessors | Corresponding getters/setters; focused defaults and round-trip tests, differential `harness_defaults`/`harness_config_roundtrip` |
| `agent-harness.ts:529-534` | `close` marks the harness closed | `close`; closed rejection tests |

## Internal branches

| TypeScript source | Observable contract | Rust proof |
|---|---|---|
| `agent-harness.ts:310-319` | Constructor defaults: thinking `off`; active tools default to tool names; empty resources/stream options; retry `{false,0,1000}`; compaction `{true,16384,20000}`; queue modes `one-at-a-time` | `defaults_match_the_typescript_constructor`, `active_tools_default_to_tool_names`, differential `harness_defaults` |
| `agent-harness.ts:312-313` | `activeToolNames` wins over tool names; both are copied | `active_tools_default_to_tool_names`, focused copy test |
| `agent-harness.ts:316-318` | Resources copy only `skills`/`promptTemplates`; absent stays absent | focused copy test and differential `harness_defaults` |
| `agent-harness.ts:320-330` | `create` queries one record without a lane filter and rejects on any record | focused restore test and differential `harness_create_restore` |
| `agent-harness.ts:449-455` | `setTools` without active names resets active tools to the new tool names | `active_tools_default_to_tool_names` |
| `agent-harness.ts:546-548` | `close` does not disable configuration accessors | focused tests read state after close in the rejection test; configuration behavior unchanged |

## Gaps and discrepancies

- **Effectful differential scenarios are impossible against the pinned source.**
  The requested multi-turn faux-provider runs, tool calls, steering/follow-up
  queues, compaction triggers, aborts, and JSONL session persistence cannot be
  driven through `AgentHarness`: every such method rejects before touching a
  provider or session. The differential covers everything the source actually
  exposes: creation, defaults, configuration round-trips, all unavailable and
  closed rejections, hooks/events rejection, session sharing, and telemetry.
  The event bus, reducer, sessions, compaction, and tools are already ported and
  differentially proven in their own slices.
- **`AgentLane` method surface is not ported.** The scaffold never constructs a
  lane value, and `lane()`/`createLane()` reject. `AgentLane` remains a marker
  trait so `lane()` and `CreateLaneResult` keep a nominal target type. This
  mirrors the earlier `SessionTree` decision.
- **Nested `Result` channels.** TypeScript distinguishes a returned
  `ResultValue` from a rejected promise. Rust has one `Result`, so methods
  return `Result<RunResult, HarnessUnavailable>` where `RunResult` is itself
  `Result<RunResultValue, RunRejected>`. No behavior is lost; callers observe
  the same two failure channels.
- **Flattened intersections.** `{ runId } & RunOutcome` becomes
  `RunResultValue { run_id, outcome }`; `ResumeOutcome` nests its operation
  outcome because Rust enum variants cannot flatten. These values are
  unreachable in the scaffold.
- **Typed input exclusions.** `StreamOptionsPatch` cannot represent an explicit
  per-field `undefined` distinct from omission; `runWhenIdle` accepts a
  synchronous callback (never invoked); hook handlers and event listeners must
  return a boxed future (sync handlers can return an immediately-ready future).
  These follow the typed-core decision and do not affect the scaffold.
- **`HarnessTool` composition.** The TypeScript intersection is modeled as
  `HarnessTool { tool, replay }`; a caller must supply a real `AgentTool`, so the
  source test's cast of a name/label-only object is not representable.
- **Ignored options.** `models`, `toolContext`, `systemPrompt`, `drive`,
  `toolExecution`, `toProviderMessages`, `entryProjectors`, and `context` are
  accepted by the source constructor and never stored. Rust keeps the option
  fields but stores none of them.
- **`HarnessFault.cause`** is typed as an `Error` in Rust instead of arbitrary
  JavaScript `unknown`; the class is never constructed by the source module.
- **Clippy-driven boxing.** `CompactionOutcome::Completed.entry` and
  `NavigationOutcome::Completed.summary_entry` are boxed, and `ResumeOutcome`
  boxes its nested outcome, to keep enum sizes uniform. Type-level only.
