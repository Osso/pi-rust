# Built-in tools index

Frozen: `packages/coding-agent/src/core/tools/index.ts` at `4f9e6f8a4`.
Rust: `crates/pi-coding-agent-tools/src/core/tools/mod.rs` and its public tool submodules.

| Frozen lines | Symbols/branches | Proof |
|---|---|---|
| 1–132 | Tool factories/types and truncation/file-mutation exports | Public Rust tool submodules, shared truncation exports; per-module traceability |
| 166–229 | ToolName, allToolNames, DEFAULT_ACTIVE_TOOL_NAMES, fixed source order | `differential_tools_index::tools_index_factories_match_the_pinned_typescript_source`, `tool_name_round_trips_the_source_names` |
| 231–240 | ToolsOptions (read/bash/edit/write/search/codeIndex operation injection) | Native option records; factory differential and injected-code-index availability proof |
| 243–286 | createToolDefinition dispatches all 19 names | Full definition metadata/schema/renderer differential |
| 288–331 | createTool: native direct tools and context-free coordination definitions | Native factory name/order proof plus `standalone_tool_factories_preserve_undefined_context` compares all nine coordination results/errors to frozen |
| 333–376 | coding/read-only/all definition groups | Definition factory differential compares all groups and object key order |
| 378–387 | createToolDefinitionsForAvailableExecutables: injected operations bypass lookup; otherwise remove only outline/symbol/references when code-index unavailable | `executable_availability_filters_only_noninjected_code_index_tools`: controlled PATH subprocesses with missing/non-executable/executable file and both injection states, direct frozen differential |
| 389–432 | coding/read-only/all executable tool groups | `tool_factories_return_the_source_names_in_order` |

Removed old-only constrainedSampling propagation: PI_EXPERIMENTAL=1 does not add metadata in frozen; `tool_definitions_do_not_add_removed_experimental_sampling` runs the real frozen oracle under that flag and compares all 19 definitions' absent sampling metadata.

Optional-context/preparation carrier details and native-extension exclusions: [tool-definition-wrapper](tool-definition-wrapper.md). Windows executable rules are the approved gap in [executable](executable.md).

Typed ToolName excludes the switch default's invalid runtime name. ToolName::from_name returns None for unknown input. Built-in consumers only read the frozen name collections; native owned copies/constant enumeration follow the approved owned-value deviation.

AgentSession consumer integration (frozen agent-session.ts:5551–5579) is a separate bounded commit with real registry/deleted-cwd behavioral tests. No source bugs classified.
