# coding-agent tool-definition-wrapper traceability

Pinned source: `packages/coding-agent/src/core/tools/tool-definition-wrapper.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

Rust: `pi_coding_agent::core::tools::tool_definition_wrapper`.

The TypeScript module converts a `ToolDefinition` (extensions API) into an `AgentTool` and caps
tool output. Rust keeps both directions: `wrap_definition`/`wrap_definitions` (definition →
`AgentTool`, the source direction) and `create_tool_definition_from_agent_tool` (reverse).

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `tool-definition-wrapper.ts:17-20` | `createFullOutputPath` = `tmpdir()/pi-tool-<16 hex>.log` | `create_full_output_path`; `oversized_results_spill_the_full_text_and_keep_the_tail` asserts the written path and file contents. |
| `tool-definition-wrapper.ts:22-28` | `mergeOutputDetails` spreads object details and appends `truncation` + `fullOutputPath`; non-object details are replaced | `merge_output_details`; spill test asserts both keys. Array/function details are excluded by the typed JSON-value API. |
| `tool-definition-wrapper.ts:30-40` | `hasExistingOutputDetails` is true when the details object has `fullOutputPath` or `truncation` | `has_existing_output_details`; `existing_output_details_are_never_recapped`. |
| `tool-definition-wrapper.ts:42-71` | `capToolResultOutput` normalizes missing content, joins text blocks with `\n`, tail-truncates, spills the full text once per call, and returns the tail plus `[Showing last … of …. Full output: …]`; `writeFileSync` throws | `cap_tool_result_output -> Result<AgentToolResult, ToolError>`; `small_results_pass_through_unchanged`, `oversized_results_spill_the_full_text_and_keep_the_tail`, `spill_write_failure_returns_the_node_error`. A partial-update spill failure is recorded in `ToolOutputSpill::error` and returned from the final `execute` (see [throw-audit](throw-audit.md)). |
| `tool-definition-wrapper.ts:73-99` | `createToolContext` / `createToolExecutionContext` add `toolExecutionAgentId` and `toolExecutionStartedAt` | `create_tool_execution_context`; `wrapped_definitions_inject_execution_identity_and_cap_output`. |
| `tool-definition-wrapper.ts:101-137` | `wrapToolDefinition` forwards name/label/description/parameters/constrainedSampling/prepareArguments/prepareExecution/executionMode and caps partial updates plus the final result with one shared spill | `DefinitionTool`, `wrap_definition`; `wrapped_definitions_forward_prepare_arguments_and_prepare_execution`, `wrapped_definitions_inject_execution_identity_and_cap_output`. `prepareArguments`/`prepareExecution` are fallible on `AgentTool` (see [agent-loop](agent-loop.md)); errors propagate to the source boundaries. |
| `tool-definition-wrapper.ts:139-144` | `wrapToolDefinitions` maps definitions | `wrap_definitions`. |
| `tool-definition-wrapper.ts:146-160` | `createToolDefinitionFromAgentTool` synthesizes a `ToolDefinition` | `create_tool_definition_from_agent_tool`; forwards `prepareArguments` and `prepareExecution` like the source; used by every tool's `create_*_tool_definition`. Proof: `definitions_from_agent_tools_forward_prepare_arguments`, `edit_definition_forwards_prepare_arguments`. |

Differential: not applicable for the module itself (random temp path). The `prepareArguments`
catch boundary is compared against `packages/agent/src/agent-loop.ts` by the
`prepare_arguments_error` scenario in `differential_agent_loop`.

## Discrepancies

- `createToolContext` returns `undefined` when no factory is supplied; the typed Rust API requires a
  `ToolContextFactory`, so `create_tool` supplies one for definition-based tools.
- `createToolDefinitionFromAgentTool` always sets `prepareExecution`; the `AgentTool` default
  returns `None`, so the observable result matches the source's conditional spread.
- `DefinitionTool::execute` always passes a callback to the definition because `AgentTool::execute`
  has no optional update callback.

## Q5 re-pin audit (2026-10-23, in progress)

- Frozen lines 22–28 spread array indices into capped details: `differential_tool_definition_wrapper::result_cap_matches_frozen_for_json_details_and_raw_text` compares scalar/object/array/null-key metadata and exact raw UTF-16 tail plus encoded spill bytes. Random path text alone is normalized.
- Frozen lines 114–121 allocate spill state per invocation. `wrapped_agent_tools_keep_spills_local_to_each_execution` reproduces the old cross-call file reuse and proves separate spill files; partial and final outputs still share a file within one call.
- Frozen lines 109–112 forward preparation; `wrapped_agent_tools_forward_prepare_execution_failure` covers the native direct-tool adapter.
- Frozen lines 101–131 and 151–174 do not copy `constrainedSampling`. Removed the obsolete metadata forwarding in both directions; `frozen_wrappers_do_not_copy_old_constrained_sampling_metadata` proves absence.
- Frozen lines 19–21 use eight random bytes, not a sliced UUID with forced version bits. `spill_names_use_random_bytes_without_a_uuid_version_nibble` exercises the filename contract. Temp paths use shared Node OS-byte decoding.
- Main cleared a fallible `AgentToolUpdateCallback` change to reproduce synchronous update-time spill failures. Pending RED/GREEN catch-versus-propagate proof; module coverage remains todo until that lands.
