# extensions/wrapper.ts

Frozen `4f9e6f8a4` (37 lines). Native `crates/pi-coding-agent-tools/src/core/extensions/wrapper.rs`.

| Frozen export/branch | Native / proof |
|---|---|
| wrapRegisteredTool, wrapRegisteredTools | wrap_registered_tool(s), one wrapper per definition; shared context_factory reads current runner |
| builtin source skips clone wrappers | all native result/partial/preparation values are already owned; approved owned-value semantics preserve the trusted builtin path without a second wrapper |
| non-builtin prepareExecution returned value detached | native owned preparation result; shared adapter preserves prepare failures |
| non-builtin onUpdate partial and execute return detached | native owned AgentToolResult/JsValue; no JS alias exists to mutate retained values |
| wrapToolDefinition delegation | same shared DefinitionTool handles execution identity/context, argument prep and capping |

Current proof: `cargo test -j3 -p pi-coding-agent --test extensions wrapped_definitions_`, 3 GREEN at 1dcf30994 (`/tmp/ext-api-wrapper.log`). Existing `wrapper_does_not_inject_old_added_tool_names` covers deletion of source-absent added-tool diff; frozen wrapper now contains only isolation/delegation, not active-tool-name tracking. The old trace described retired behavior and is replaced, not preserved as current evidence.

Native-only `context_tool_definition` is the existing ContextTool trait adapter for bundled coordination tools. It does not change frozen wrapper behavior. Gap: adversarial JS non-cloneable values/aliasing follow typed/owned/Rust-native approvals.
