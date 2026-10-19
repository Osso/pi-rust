# coding-agent tools/index.ts traceability

Pinned source: `packages/coding-agent/src/core/tools/index.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools` (`mod.rs`).

| TypeScript export | Rust | Tests |
|---|---|---|
| `ToolName` | `ToolName` enum (`as_str`, `from_name`) | `tool_name_round_trips_the_source_names` |
| `allToolNames` | `ALL_TOOL_NAMES` | differential `27960` (`allNames`) |
| `DEFAULT_ACTIVE_TOOL_NAMES` | `DEFAULT_ACTIVE_TOOL_NAMES` | differential `27960` (`names`) |
| `ToolsOptions` | `ToolsOptions` | differential `27960` |
| `createToolDefinition` | `create_tool_definition` | differential `27960` (all 19 definitions: name/label/description, prompt metadata, approval metadata, parameters, execution mode, render shell, renderer presence) |
| `createTool` | `create_tool` | `tool_factories_return_the_source_names_in_order` |
| `createCodingToolDefinitions` | `create_coding_tool_definitions` | differential `27960` (`coding`) |
| `createReadOnlyToolDefinitions` | `create_read_only_tool_definitions` | differential `27960` (`readOnly`) |
| `createAllToolDefinitions` | `create_all_tool_definitions` | differential `27960` (`allKeys`) |
| `createCodingTools` | `create_coding_tools` | `tool_factories_return_the_source_names_in_order` |
| `createReadOnlyTools` | `create_read_only_tools` | same |
| `createAllTools` | `create_all_tools` | same |

Differential: `oracle/coding-agent-tools-index-oracle.ts` (request `27960`) loads the real
`tools/index.ts` and compares every factory output and the name lists.

## Deviations

- `createTool`/`createAllTools`/`createCodingTools`/`createReadOnlyTools` take a
  `ToolContextFactory`. The definition-based tools need the extension context that the source
  passes as `undefined` when no factory is supplied; the typed Rust API requires it. The
  file/coding tools ignore the factory.
- `createTool`'s `default: throw new Error("Unknown tool name: …")` is unreachable: `ToolName`
  excludes unknown names (typed-API exclusion).

## Discrepancies

- None observed.
