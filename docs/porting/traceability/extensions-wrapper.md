# extensions/wrapper.ts → `core::extensions::wrapper`

Source: `packages/coding-agent/src/core/extensions/wrapper.ts` (46 lines).
Rust: `crates/pi-coding-agent/src/core/extensions/wrapper.rs`.

| Source | Rust | Tests |
| --- | --- | --- |
| `wrapRegisteredTool` | `wrap_registered_tool` | `wrapper_reports_tools_added_during_execution` |
| `wrapRegisteredTools` | `wrap_registered_tools` | same |
| active-tools diff / `addedToolNames` merge | `WrappedExtensionTool::execute` | same |
| `wrapToolDefinition` delegation | `wrap_definition_tool` composes `tool_definition_wrapper::wrap_definition` with `runner.createContext()` | `wrapped_definitions_*`; `WrappedExtensionTool` delegates `prepareArguments`/`prepareExecution` to the adapter |

## Gaps

- None. `prepareArguments`, `prepareExecution`, execution-context injection, and result capping
  come from the shared `DefinitionTool` adapter; `WrappedExtensionTool` only adds the
  added-tool-name diff around the wrapped execution and delegates the prepare hooks.

## Discrepancies

None; the added-tool diff matches the source, including "all previously active tools still
active" and de-duplicated append order.
