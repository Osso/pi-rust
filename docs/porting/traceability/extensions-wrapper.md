# extensions/wrapper.ts → `core::extensions::wrapper`

Source: `packages/coding-agent/src/core/extensions/wrapper.ts` (46 lines).
Rust: `crates/pi-coding-agent/src/core/extensions/wrapper.rs`.

| Source | Rust | Tests |
| --- | --- | --- |
| `wrapRegisteredTool` | `wrap_registered_tool` | `wrapper_reports_tools_added_during_execution` |
| `wrapRegisteredTools` | `wrap_registered_tools` | same |
| active-tools diff / `addedToolNames` merge | `WrappedExtensionTool::execute` | same |
| `wrapToolDefinition` delegation | `WrappedExtensionTool` adapter | same |

## Gaps

- `tool-definition-wrapper.ts` lives in `core::tools::tool_definition_wrapper`
  (matching the source path), not in this module. The wrapper calls
  `ToolDefinition.execute` directly with the runner context and passes
  `tool_execution_started_at` / `tool_execution_agent_id`.
- `prepareArguments` is not applied by the wrapper (agent-core default identity)
  until the tools wrapper lands.
- `prepareExecution` errors map to `None` because `AgentTool::prepare_execution`
  cannot return an error.

## Discrepancies

None beyond the gaps above; the added-tool diff matches the source, including
"all previously active tools still active" and de-duplicated append order.
