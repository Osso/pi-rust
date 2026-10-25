# coding-agent tool-definition-wrapper traceability

Frozen: `packages/coding-agent/src/core/tools/tool-definition-wrapper.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Rust: `crates/pi-coding-agent-tools/src/core/tools/tool_definition_wrapper.rs`.

| Frozen lines | Behavior | Proof |
|---|---|---|
| 19–29 | Eight random bytes form the spill basename; spread object/array details before appending metadata | `tool_definition_wrapper::spill_names_use_random_bytes_without_a_uuid_version_nibble`; `differential_tool_definition_wrapper::result_cap_matches_frozen_for_json_details_and_raw_text` (array/scalar/object details, raw UTF-16, exact Node-encoded spill bytes; only random paths normalized) |
| 31–39 | Existing `fullOutputPath` or `truncation` keys bypass capping regardless of value | `existing_output_details_are_never_recapped`; differential includes `truncation: null` |
| 41–67 | Join text entries, cap tail, spill full UTF-8 bytes, replace content and merge metadata; synchronous filesystem failures | `small_results_pass_through_unchanged`, `oversized_results_spill_the_full_text_and_keep_the_tail`, `spill_write_failure_returns_the_node_error`; lossless spill and differential fixtures |
| 69–93 | Context factory + per-execution identity without mutating the base | `extensions::wrapped_definitions_inject_execution_identity_and_cap_output`; optional context carrier; standalone index differential covers all nine coordination factories with None |
| 96–123 | Forward preparation/execution metadata; cap updates and final result with one invocation-local spill | `extensions::wrapped_definitions_forward_prepare_arguments_and_prepare_execution`; `wrapped_agent_tools_forward_prepare_execution_failure`; `wrapped_tools_cap_partial_updates_and_reuse_one_spill_file`; `wrapped_agent_tools_keep_spills_local_to_each_execution` |
| 114–125 | Update spill throws at the update call, allowing definition catch or propagation, not deferred failure | `differential_tool_definition_wrapper::update_spill_failure_is_synchronous_and_catchable`: real TS definition versus Rust definition, failing spill path, recovery versus propagation and before/after execution trace |
| agent-loop.ts:845–850 | Execution rejection becomes Error.message or String(value) only at the runtime adapter | `project_callback_tool_error`; actual frozen/native direct-factory + real-agent-loop seven-case histories in `differential_question_rejections`; existing Error text assertions retained |
| 126–131 | Map definitions in order | `wrap_definitions`; same per-definition adapter as the standalone index factory matrix |
| 139–157 | Reverse adapter preserves preparation/execution; removed old-only constrainedSampling passthrough | `definitions_from_agent_tools_forward_prepare_arguments`; `frozen_wrappers_do_not_copy_old_constrained_sampling_metadata` |

## Assigned tmpdir authority correction

Frozen `tool-definition-wrapper.ts:21` explicitly calls `join(tmpdir(), pi-tool-...)`. Its native getter alone is routed through shared [POSIX tmpdir](../../specs/node-tmpdir.md); random-byte naming and cap/error logic remain unchanged. `differential_tmpdir_consumers::wrapper_spill_uses_mutable_node_tmpdir` compares the real frozen wrapper's full file bytes, parent directory and basename shape across four TMPDIR/TMP/TEMP/trailing-slash cases. `tmpdir-wrapper-red.log` proves the previous std getter chose a different root; All four histories GREEN at ea60354a, with retained synchronous catch/timer cases (`tmpdir-consumers-green.log`); [Integration checkpoint](node-os.md#tmpdir-checkpoint-eabd39b5).

## Shared callback correction

`AgentToolUpdateCallback` returns `Result<(), ToolError>`. Wrappers return cap/write errors synchronously; definitions can catch or propagate them. The old `ToolOutputSpill::error` deferred-error channel was removed. Shared Pyrun progress reporting propagates callback failures through its existing runner rejection boundary. Initial/final Bash updates propagate errors; event/timer updates use the existing process uncaught authority. `differential_tool_definition_wrapper::timer_update_failure_uses_the_uncaught_process_boundary` compares actual frozen/Rust child processes: exit 1 and `Error: timer update failed`, never execute recovery. V8 stack frames are excluded under the approved stack gap.

## Optional context and hook presence

`ToolExecute`/`PrepareExecution` carry `Option<ExtensionContext>`; standalone factories deliver None. Factory-backed wrappers clone the base and overwrite the execution identity only for execution, not preparation. `AgentTool.prepare_execution()` returns one optional callback; absent preparation never invokes the context factory (`tool_definition_wrapper::default_agent_tool_preparation_does_not_create_context`).

Caller audit: frozen `tools/index.ts:311–327,422–430` wraps coordination definitions without a factory. `read.ts:504`, `write.ts:266`, `edit.ts:436`, `bash.ts:841`, `grep.ts:394`, `find.ts:374`, `ls.ts:320`, `code-index.ts:320–328` do likewise for native direct tools. Frozen `extensions/wrapper.ts:8–19` supplies the runner factory. `differential_tools_index::standalone_tool_factories_preserve_undefined_context` proves the None path across all nine index coordination tools, including successful end_turn and exact missing-context errors.

Registered Rust-native extension callbacks require Some from their runner; None is unreachable through built-in callers. `core/extensions/wrapper.rs::wrap_definition_tool` supplies Some(factory), and `DefinitionTool::execute` maps that factory to Some(context). `require_extension_context` returns an explicit HandlerError if a native embedder violates that boundary, never panics or invents a context. This is under the approved Rust-native extension deviation (main confirmed 2026-10-23); no JS undefined-property errors are claimed for native extensions. Native standalone built-ins do not use that helper and match frozen None behavior.

## Approved ingress (c) gap (2026-10-24)

| Rejection value | Frozen outcome | Explicit native outcome / proof |
|---|---|---|
| Own `toString` throws during rejection text conversion (JSON fixture `{toString:null}`) | TypeError escapes the execution catch; real Agent.prompt catches it, resolves, ends streaming and emits an assistant error with no tool result | Message-only ToolError cannot carry a loop-fatal outcome: return an explicit ToolError with exact `Cannot convert object to primitive value`, producing one tool-result message. Never use uncaught-process authority. Actual source process exits0; native test asserts both different shapes. [Decision/proof](../../specs/custom-ui-rejections.md#approved-gap-rejection-value-whose-own-tostring-throws) |

Typed JSON details exclude prototype inheritance/function values and invalid runtime content types. Rust owned result/context copies follow the approved owned-value deviation. No source bugs classified.

2026-10-25 mechanical carrier retirement: removed the source-absent ConstrainedSampling trait/type field and its dedicated absent-metadata fixture after parent consumer deletion. Prepare hooks, labels, payloads, spill behavior and other wrapper assertions are unchanged.
