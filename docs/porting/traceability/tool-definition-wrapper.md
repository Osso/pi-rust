# coding-agent tool-definition-wrapper traceability

Frozen: `packages/coding-agent/src/core/tools/tool-definition-wrapper.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Rust: `crates/pi-coding-agent/src/core/tools/tool_definition_wrapper.rs`.

| Frozen lines | Behavior | Proof |
|---|---|---|
| 19–28 | Eight random bytes form the spill basename; spread object/array details before appending metadata | `tool_definition_wrapper::spill_names_use_random_bytes_without_a_uuid_version_nibble`; `differential_tool_definition_wrapper::result_cap_matches_frozen_for_json_details_and_raw_text` (array/scalar/object details, raw UTF-16, exact Node-encoded spill bytes; only random paths normalized) |
| 30–40 | Existing `fullOutputPath` or `truncation` keys bypass capping regardless of value | `existing_output_details_are_never_recapped`; differential includes `truncation: null` |
| 42–71 | Join text entries, cap tail, spill full UTF-8 bytes, replace content and merge metadata; synchronous filesystem failures | `small_results_pass_through_unchanged`, `oversized_results_spill_the_full_text_and_keep_the_tail`, `spill_write_failure_returns_the_node_error`; lossless spill and differential fixtures |
| 73–99 | Context factory + per-execution identity without mutating the base | `extensions::wrapped_definitions_inject_execution_identity_and_cap_output`; required factory limitation below |
| 101–131 | Forward preparation/execution metadata; cap updates and final result with one invocation-local spill | `extensions::wrapped_definitions_forward_prepare_arguments_and_prepare_execution`; `wrapped_agent_tools_forward_prepare_execution_failure`; `wrapped_tools_cap_partial_updates_and_reuse_one_spill_file`; `wrapped_agent_tools_keep_spills_local_to_each_execution` |
| 114–125 | Update spill throws at the update call, allowing definition catch or propagation, not deferred failure | `differential_tool_definition_wrapper::update_spill_failure_is_synchronous_and_catchable`: real TS definition versus Rust definition, failing spill path, recovery versus propagation and before/after execution trace |
| 134–140 | Map definitions in order | `wrap_definitions`; no separate runtime proof yet |
| 149–174 | Reverse adapter preserves preparation/execution; removed old-only constrainedSampling passthrough | `definitions_from_agent_tools_forward_prepare_arguments`; `frozen_wrappers_do_not_copy_old_constrained_sampling_metadata` |

## Shared callback correction

`AgentToolUpdateCallback` returns `Result<(), ToolError>`. Wrappers return cap/write errors synchronously; definitions can catch or propagate them. The old `ToolOutputSpill::error` deferred-error channel was removed. Shared Pyrun progress reporting propagates callback failures through its existing runner rejection boundary. Initial/final Bash updates propagate errors; event/timer updates use the existing process uncaught authority. `differential_tool_definition_wrapper::timer_update_failure_uses_the_uncaught_process_boundary` compares actual frozen/Rust child processes: exit 1 and `Error: timer update failed`, never execute recovery. V8 stack frames are excluded under the approved stack gap.

## Pending classification / proof

- Frozen omitted `ctxFactory` delivers an undefined context despite the callback's non-optional ExtensionContext type. Rust currently requires a factory. Main decision requested; coverage remains todo, not done.
- Native reverse adapters always provide preparation hooks whose defaults return unchanged arguments/None. Optional-property presence is not yet proven equivalent for consumers.
- Rust always supplies an update callback because the AgentTool trait requires one; frozen can omit it. No new deviation claimed.

No source bugs classified or approved semantic deviations introduced by Q5.
