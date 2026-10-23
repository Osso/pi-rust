# Traceability: `packages/agent-core/src/types.ts`

Reference: immutable `4f9e6f8a4`. Rust: `crates/pi-agent-core/src/agent/types.rs`.
Evidence/exclusions: [slice-1 ledger](agent-core-api-repin.md). Complete with approved owned-value/event-snapshot and typed-input exclusions. Native human text, arbitrary argument/details JSON and reason-bearing signals are ported, not excluded.

| Frozen export | Rust representation | Proof |
|---|---|---|
| StreamFn | Arc callback returning boxed `Result<AssistantMessageEventStream, AgentError>` future | native acquisition rejection regression, frozen rejection differential, [coding consumer proof](stream-acquisition.md) |
| ToolExecutionMode, QueueMode, ThinkingLevel | enums re-exported from harness/types through agent/types and agent | respective enum tests; queue/tool-mode differentials |
| AgentToolCall | pi-ai ToolCall alias | tool execution/restoration fixtures |
| BeforeToolCallResult | block/reason option fields, re-exported harness type | before_tool_call_result.rs; blocked/default-reason loop tests |
| AfterToolCallResult | content/details/is_error/terminate options | after-hook tests; no old usage field |
| BeforeToolCallContext, AfterToolCallContext | named structs; before args use Arc<Mutex<Value>> | before argument mutation and raw-call/validated-argument regression |
| ShouldStopAfterTurnContext, PrepareNextTurnContext | struct and alias | replaced-context prompt/restoration tests |
| AgentLoopTurnUpdate | optional context/model/thinking_level | prepare_next_turn differential and behavior tests |
| AgentLoopConfig | named fields + nested SimpleStreamOptions | forwarded request/queue/hook tests; stop hook has context only |
| Queue message callbacks | `MessageQueueHook` returns `Result<Vec<AgentMessage>, AgentError>` | frozen steering/follow-up rejection differentials; native-unit event-boundary regression |
| CustomAgentMessages, AgentMessage | declaration merging becomes Custom(JsonObject); standard enum variants | agent_types.rs standard/custom round trips |
| AgentState | owned snapshot, access through Agent methods | state and pending insertion-order tests; built-in mutation consumers use explicit setters, as audited in agent.md |
| AgentToolResult | content/details/is_error/terminate | serde result tests; removed usage/added_tool_names; coding consumer migrations |
| AgentToolUpdateCallback | Arc callback | partial updates, ignore-after-settle, abort tests |
| AgentToolExecutionContext | started_at/optional agent_id, re-exported harness type | agent_tool_execution_context.rs, prepareExecution fixtures |
| AgentTool | trait: schema/label, optional argument/execution preparation/mode, execute | mode/preparation/tool error/update tests |
| AgentContext | system_prompt/messages/tools | request conversion and restored context tests |
| AgentEvent | tagged enum, camelCase payload fields | agent_types.rs wire fixtures and exact differential event streams |

Rust helper aliases express callback types, overloads and structural interfaces; no global stream registry remains. ToolResultMessage is a separate shared pi-ai type: its representation was not changed by this slice. Shared native TextContent/JsValue details retain UTF-16; nullish after-hook selection remains distinct from supplied null. AbortSignal is the shared reason-bearing carrier, not a token alias.

## Native tool text (2026-10-22)

Frozen `types.ts:383–385` inherits the human `Tool.description` and adds `label: string`. `AgentTool` getters now return owned `JsString`; coding definition/context adapters and tool-info records retain that authority. `lossless_agent::agent_loop_keeps_tool_text_details_and_arguments_through_message_wire_reload` asserts unmatched description/label units in actual provider context; `tool_definition_wrapper::definitions_from_agent_tools_forward_prepare_arguments` asserts the same units survive conversion to extension definitions. RED: `tool-native-red.log`; GREEN: `tool-native-green.log` (4 native + 3 frozen matrix groups), `tool-coding-green2.log` (7 adapter tests). Logs in `/tmp/pi-rust-repin-delta-agent-core/`. This closes the human-text carriers; remaining representation classifications follow the explicit owned-value, event-snapshot and typed-input approvals.
