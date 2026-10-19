# Traceability: `packages/agent/src/types.ts`

Rust: `crates/pi-agent-core/src/agent/types.rs`. Tests: `crates/pi-agent-core/tests/agent_types.rs`.

## Exports

| TypeScript export | Rust item | Tests | Notes |
|---|---|---|---|
| `StreamFn` | `agent::StreamFn` | `stream_fn.rs` | `Arc<dyn Fn(Model, Context, SimpleStreamOptions) -> BoxFuture<'static, AssistantMessageEventStream>>`; the TS `Promise<stream>` return is folded into the future. |
| `ToolExecutionMode` | `harness::types::ToolExecutionMode` | `tool_execution_mode.rs` | Reused from the harness port; not duplicated. |
| `QueueMode` | `harness::types::QueueMode` | `queue_mode.rs` | Reused from the harness port. |
| `AgentToolCall` | `agent::AgentToolCall = pi_ai::types::ToolCall` | `agent_types.rs` | Alias. |
| `BeforeToolCallResult` | `harness::types::BeforeToolCallResult` | `before_tool_call_result.rs` | Reused; `block`/`reason`/`terminate` are `Option` fields for TS optional properties. |
| `AfterToolCallResult` | `agent::AfterToolCallResult` | `agent_types.rs` | Field-by-field override record. |
| `BeforeToolCallContext` | `agent::BeforeToolCallContext` | `agent_types.rs` | |
| `AfterToolCallContext` | `agent::AfterToolCallContext` | `agent_types.rs` | |
| `ShouldStopAfterTurnContext` | `agent::ShouldStopAfterTurnContext` | `agent_types.rs` | |
| `AgentLoopTurnUpdate` | `agent::AgentLoopTurnUpdate` | `agent_loop.rs` | |
| `PrepareNextTurnContext` | `agent::PrepareNextTurnContext` | `agent_loop.rs` | Alias of `ShouldStopAfterTurnContext`, matching the TS interface extension. |
| `AgentLoopConfig` | `agent::AgentLoopConfig` | `agent_loop.rs` | `SimpleStreamOptions` is held in `stream_options`; hooks are `Option<Arc<dyn Fn .. -> BoxFuture>>`. |
| `ThinkingLevel` | `harness::types::ThinkingLevel` | `thinking_level.rs` | Reused; `reasoning_for` maps it onto `pi_ai::types::ThinkingLevel` (`off` → `None`). |
| `CustomAgentMessages` | n/a (type-level) | `agent_types.rs` | Declaration merging has no Rust equivalent; custom messages are `AgentMessage::Custom(JsonObject)`. |
| `AgentMessage` | `agent::AgentMessage` | `agent_types.rs` | Untagged enum; `Custom` keeps the full object including `role` and round-trips verbatim. |
| `AgentState` | `agent::AgentState` | `agent.rs` | Snapshot returned by `Agent::state()`; runtime fields are copied, not live accessors. `pending_tool_calls` is an `IndexSet`, preserving TypeScript `Set` insertion order. |
| `AgentToolResult` | `agent::AgentToolResult` | `agent_types.rs` | `details` is `Option<Value>` so TS `undefined` is omitted from JSON; `terminate`/`isError` stay optional. |
| `AgentToolUpdateCallback` | `agent::AgentToolUpdateCallback` | `agent_loop.rs` | Ignored after settlement by the loop's accepting-updates flag. |
| `AgentToolExecutionContext` | `harness::types::AgentToolExecutionContext` | `agent_tool_execution_context.rs` | Reused; `started_at` stays `f64` per the existing port contract. |
| `AgentTool` | `agent::AgentTool` | `agent_loop.rs` | Dyn-compatible trait; `parameters()` returns the JSON Schema `Value`; async members return `BoxFuture<'static, _>`. `prepare_arguments` and `prepare_execution` return `Result` (TS throws propagate to the loop's catch / abort). |
| `AgentContext` | `agent::AgentContext` | `agent_types.rs` | Tools are `Option<Vec<Arc<dyn AgentTool>>>`. |
| `AgentEvent` | `agent::AgentEvent` | `agent_types.rs`, `agent_loop.rs`, `agent.rs` | Internally tagged with snake_case type names and camelCase fields; `assistant_message_event` is boxed (clippy large-variant). |

## Branches and exclusions

- `AgentMessage` role discrimination: standard roles deserialize into typed variants; everything else falls through to `Custom` (`agent_message_round_trips_custom_objects_verbatim`).
- Optional result fields serialize only when present, matching `JSON.stringify` dropping `undefined` (`agent_tool_result_omits_undefined_fields`).
- `AgentState` accessor properties (`set tools`, `set messages` copy on assignment) become explicit `Agent` setters that clone the vector; getters return copies. This is a Rust API shape adaptation, not a behavior change.
- Typed exclusion: `AgentTool::parameters` requires a JSON Schema `Value`; JavaScript callers passing a non-object are not representable.
