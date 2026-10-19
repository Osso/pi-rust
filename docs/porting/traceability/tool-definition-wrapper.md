# coding-agent tool-definition-wrapper traceability

Pinned source: `packages/coding-agent/src/core/tools/tool-definition-wrapper.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::tool_definition_wrapper`.

The TypeScript module converts a `ToolDefinition` (extensions API) into an `AgentTool`. The extensions `ToolDefinition` type is owned by the in-flight `ca-extensions-api` slice, so the Rust port applies the same result capping to an existing `AgentTool` instead of converting between the two types.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `tool-definition-wrapper.ts:17-20` | `createFullOutputPath` = `tmpdir()/pi-tool-<16 hex>.log` | `create_full_output_path`; `oversized_results_spill_the_full_text_and_keep_the_tail` asserts the written path and file contents. |
| `tool-definition-wrapper.ts:22-28` | `mergeOutputDetails` spreads object details and appends `truncation` + `fullOutputPath`; non-object details are replaced | `merge_output_details`; spill test asserts both keys. Array/function details are excluded by the typed JSON-value API. |
| `tool-definition-wrapper.ts:30-40` | `hasExistingOutputDetails` is true when the details object has `fullOutputPath` or `truncation` | `has_existing_output_details`; `existing_output_details_are_never_recapped`. |
| `tool-definition-wrapper.ts:42-71` | `capToolResultOutput` normalizes missing content, joins text blocks with `\n`, tail-truncates, spills the full text once per call, and returns the tail plus `[Showing last … of …. Full output: …]`; `writeFileSync` throws | `cap_tool_result_output -> Result<AgentToolResult, ToolError>`; `small_results_pass_through_unchanged`, `oversized_results_spill_the_full_text_and_keep_the_tail`, `spill_write_failure_returns_the_node_error`. A partial-update spill failure is recorded in `ToolOutputSpill::error` and returned from the final `execute` (see [throw-audit](throw-audit.md)). |
| `tool-definition-wrapper.ts:73-99` | `createToolContext` / `createToolExecutionContext` add `toolExecutionAgentId` and `toolExecutionStartedAt` | `CappedTool` forwards `AgentToolExecutionContext` unchanged; the runner context already carries both fields when tool execution provides them. |
| `tool-definition-wrapper.ts:101-137` | `wrapToolDefinition` forwards name/label/description/parameters/constrainedSampling/prepareArguments/executionMode and caps partial updates plus the final result with one shared spill | `CappedTool`; `wrapped_tools_cap_partial_updates_and_reuse_one_spill_file`. `prepare_execution` is forwarded by the trait default only. |
| `tool-definition-wrapper.ts:139-144` | `wrapToolDefinitions` maps definitions | Not applicable: Rust callers wrap each `AgentTool` with `wrap_agent_tool`. |
| `tool-definition-wrapper.ts:146-160` | `createToolDefinitionFromAgentTool` synthesizes a `ToolDefinition` | `create_tool_definition_from_agent_tool` in this module; used by every tool's `create_*_tool_definition`. |

Differential: not applicable. The module's only nontrivial output is a random temp-file path plus `truncateTail`, whose behavior is already proven by `differential_coding_agent_truncate`; the Rust tests assert the same observable result shape and file contents.

## Discrepancies

- None observed.
