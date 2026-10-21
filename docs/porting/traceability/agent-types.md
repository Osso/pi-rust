# Traceability: `packages/agent-core/src/types.ts`

Reference: immutable `4f9e6f8a4`. Rust: `crates/pi-agent-core/src/agent/types.rs`.
Evidence/exclusions: [slice-1 ledger](agent-core-api-repin.md). Status todo: shared value/reference fidelity remains unclassified.

| Frozen export | Rust representation | Proof |
|---|---|---|
| StreamFn | Arc callback returning boxed assistant stream future | agent/loop stream acquisition and error/abort tests |
| ToolExecutionMode, QueueMode, ThinkingLevel | enums re-exported from harness/types through agent/types and agent | respective enum tests; queue/tool-mode differentials |
| AgentToolCall | pi-ai ToolCall alias | tool execution/restoration fixtures |
| BeforeToolCallResult | block/reason option fields, re-exported harness type | before_tool_call_result.rs; blocked/default-reason loop tests |
| AfterToolCallResult | content/details/is_error/terminate options | after-hook tests; no old usage field |
| BeforeToolCallContext, AfterToolCallContext | named structs; before args use Arc<Mutex<Value>> | before argument mutation and raw-call/validated-argument regression |
| ShouldStopAfterTurnContext, PrepareNextTurnContext | struct and alias | replaced-context prompt/restoration tests |
| AgentLoopTurnUpdate | optional context/model/thinking_level | prepare_next_turn differential and behavior tests |
| AgentLoopConfig | named fields + nested SimpleStreamOptions | forwarded request/queue/hook tests; stop hook has context only |
| CustomAgentMessages, AgentMessage | declaration merging becomes Custom(JsonObject); standard enum variants | agent_types.rs standard/custom round trips |
| AgentState | owned snapshot, access through Agent methods | state and pending insertion-order tests; reference semantics gap |
| AgentToolResult | content/details/is_error/terminate | serde result tests; removed usage/added_tool_names; coding consumer migrations |
| AgentToolUpdateCallback | Arc callback | partial updates, ignore-after-settle, abort tests |
| AgentToolExecutionContext | started_at/optional agent_id, re-exported harness type | agent_tool_execution_context.rs, prepareExecution fixtures |
| AgentTool | trait: schema/label, optional argument/execution preparation/mode, execute | mode/preparation/tool error/update tests |
| AgentContext | system_prompt/messages/tools | request conversion and restored context tests |
| AgentEvent | tagged enum, camelCase payload fields | agent_types.rs wire fixtures and exact differential event streams |

Rust helper aliases express callback types, overloads and structural interfaces; no global stream registry remains. ToolResultMessage is a separate shared pi-ai type: its representation was not changed by this slice. TextContent and Option<Value> details representation remain unchanged; nullish after-hook selection was corrected without a representation change.
