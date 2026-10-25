# Baseline workspace failures before test consolidation

## Successor baseline recheck (2026-10-25)

Rechecked each assigned extant target separately with `cargo test -j3 -p <package> --test <suite> <original-module-filter>` at `146eb5622`. Current repairs retain exact frozen assertions except the explicitly approved NUL environment-name runtime gap and approved editor-scroll-width delta. The five handed-off failures remain open here; their owners provide integration proof. Retired targets are not passing tests. Original evidence below is historical.

| Package | Original target | Current status / proof |
| --- | --- | --- |
| `pi-coding-agent` | `differential_core_leaves` | Resolved: `/tmp/triage2-core-leaves-green2.log` |
| `pi-coding-agent` | `differential_export_html` | Resolved on parent baseline: `/tmp/triage2-differential_export_html.log` |
| `pi-coding-agent` | `differential_image_pipeline` | Resolved on parent baseline: `/tmp/triage2-differential_image_pipeline.log` |
| `pi-coding-agent` | `differential_package_manager_cli` | Resolved on parent baseline: `/tmp/triage2-differential_package_manager_cli.log` |
| `pi-coding-agent` | `example_kimi_deferred_tools` | Retired: frozen-absent module/target deleted in predecessor commits |
| `pi-telemetry` | `differential_telemetry` | Retired: frozen-absent module/crate deleted in predecessor commits |
| `pi-coding-agent` | `example_qna` | Resolved: `/tmp/triage2-final-example_qna.log` |
| `pi-coding-agent` | `export_html` | Resolved on parent baseline: `/tmp/triage2-export_html.log` |
| `pi-coding-agent` | `example_input_transform_streaming` | Resolved on parent baseline: `/tmp/triage2-example_input_transform_streaming.log` |
| `pi-coding-agent` | `example_permission_gate` | Resolved on parent baseline: `/tmp/triage2-example_permission_gate.log` |
| `pi-coding-agent` | `claude_bash_hook_extension` | Resolved on parent baseline: `/tmp/triage2-claude_bash_hook_extension.log` |
| `pi-coding-agent` | `detached_pyrun_runner_process` | Resolved on parent baseline: `/tmp/triage2-detached_pyrun_runner_process.log` |
| `pi-coding-agent` | `differential_claude_bash_hook` | Resolved on parent baseline: `/tmp/triage2-differential_claude_bash_hook.log` |
| `pi-coding-agent` | `host_relay_resident` | Resolved on parent baseline: `/tmp/triage2-host_relay_resident.log` |
| `pi-js-compat` | `process_env` | Resolved on parent baseline: `/tmp/triage2-process_env.log` |
| `pi-coding-agent` | `bundled_extensions_context` | Remaining: handed to agent_278 by main; `/tmp/triage2-bundled_extensions_context.log` |
| `pi-coding-agent` | `bundled_extensions_tools` | Resolved on parent baseline: `/tmp/triage2-bundled_extensions_tools.log` |
| `pi-coding-agent` | `differential_args` | Resolved on parent baseline: `/tmp/triage2-differential_args.log` |
| `pi-coding-agent` | `differential_bundled_extensions` | Resolved on parent baseline: `/tmp/triage2-differential_bundled_extensions.log` |
| `pi-coding-agent` | `differential_compaction` | Remaining: handed to agent_267 by main; `/tmp/triage2-differential_compaction.log` |
| `pi-coding-agent` | `differential_config_lossless_package` | Resolved: `/tmp/triage2-final2-differential_config_lossless_package.log` |
| `pi-coding-agent` | `differential_interactive_components` | Resolved: `/tmp/triage2-final2-differential_interactive_components.log` |
| `pi-coding-agent` | `differential_keybindings` | Resolved on parent baseline: `/tmp/triage2-differential_keybindings.log` |
| `pi-coding-agent` | `differential_openai_remote_compact` | Remaining: handed to agent_278 by main; `/tmp/triage2-differential_openai_remote_compact.log` |
| `pi-coding-agent` | `differential_slash_commands` | Resolved on parent baseline: `/tmp/triage2-differential_slash_commands.log` |
| `pi-coding-agent` | `differential_supervisor_request_loop` | Remaining: handed to agent_265 by main; `/tmp/triage2-differential_supervisor_request_loop.log` |
| `pi-coding-agent` | `differential_supervisor_service` | Remaining: handed to agent_265 by main; `/tmp/triage2-differential_supervisor_service.log` |
| `pi-coding-agent` | `differential_tree_selector` | Resolved: `/tmp/triage2-final-differential_tree_selector.log` |
| `pi-coding-agent` | `example_bash_spawn_hook` | Resolved: `/tmp/triage2-bash-spawn-green.log` |
| `pi-coding-agent` | `example_file_trigger` | Resolved on parent baseline: `/tmp/triage2-example_file_trigger.log` |
| `pi-coding-agent` | `example_reload_runtime` | Resolved on parent baseline: `/tmp/triage2-example_reload_runtime.log` |
| `pi-coding-agent` | `example_subagent` | Resolved on parent baseline: `/tmp/triage2-example_subagent.log` |
| `pi-coding-agent` | `experimental_cli` | Retired: frozen-absent module/target deleted in predecessor commits |
| `pi-coding-agent` | `main_process` | Resolved: `/tmp/triage2-suite_core_3-green.log + /tmp/triage2-final-suite_core_3.log` |
| `pi-coding-agent` | `package_manager_cli` | Resolved on parent baseline: `/tmp/triage2-package_manager_cli.log` |
| `pi-coding-agent` | `session_autoname` | Resolved on parent baseline: `/tmp/triage2-session_autoname.log` |
| `pi-tui` | `raw_editor` | Resolved: `/tmp/triage2-final-suite_interactive.log` |
| `pi-tui` | `verifier_regressions` | Resolved on parent baseline: `/tmp/triage2-verifier_regressions.log` |

Excluded: pi-ai targets; interactive lib and session_coordination_tools (already repaired); differential_version_check (agent_266). No new ignored cases were added; reported ignored cases are existing child helpers.

Baseline source: `09f10e9c073adba93dbf60eda3ffca1e4578cc23`; parent build configuration includes the incremental-only merge `bbc0ed0`. Command: `cargo test -j8 --workspace --no-fail-fast`. There are **50 failed targets**, **76 FAILED test outcomes**, and **two unfinished interactive tests** at the approved timeout. These failures precede the test-layout change. No product fix or assertion change is included here.

Evidence: `/tmp/test-consolidate-proof/before-suite.log`, `baseline-results.json`, `baseline-target-evidence.json`, and `baseline-failure-classification.json`. The sections distinguish evidenced failure mechanisms from assertion differences whose root cause remains uninvestigated; they must not be treated as verified product diagnoses.

## Missing/stale oracle inputs and framing (14 targets)

| Package | Original test target | Evidence / assignment note |
| --- | --- | --- |
| `pi-ai` | `differential_cloudflare_gateway_binding` | Missing pinned source: cloudflare-gateway-binding.ts. |
| `pi-ai` | `differential_constrained_sampling` | Missing pinned source: constrained-sampling.ts. |
| `pi-ai` | `differential_deferred_tools` | Missing pinned source: deferred-tools.ts. |
| `pi-ai` | `differential_model_catalog` | Missing pinned source: model-catalog.ts. |
| `pi-ai` | `differential_text` | Missing pinned source: text.ts. |
| `pi-coding-agent` | `differential_core_leaves` | Missing pinned source: auth.ts, command.ts, pi-manifest.ts, transport-address.ts. |
| `pi-coding-agent` | `differential_export_html` | export_fixture still copies the absent src/core/export_html/templates directory. |
| `pi-coding-agent` | `differential_image_pipeline` | Missing pinned source: tool-result-images.ts. |
| `pi-coding-agent` | `differential_package_manager_cli` | Oracle JSON line instead contains TUI control sequences. |
| `pi-coding-agent` | `differential_version_check` | Frozen api.formatVersionCheckError is not a function. |
| `pi-coding-agent` | `example_kimi_deferred_tools` | Missing pinned source: kimi-deferred-tools.ts. |
| `pi-coding-agent` | `example_qna` | Frozen utils.getModelRuntime is not a function. |
| `pi-coding-agent` | `export_html` | export_fixture still copies the absent src/core/export_html/templates directory. |
| `pi-telemetry` | `differential_telemetry` | Missing pinned source: index.ts. |

## Unsupported regex syntax (2 targets)

| Package | Original test target | Evidence / assignment note |
| --- | --- | --- |
| `pi-coding-agent` | `example_input_transform_streaming` | JS RegExp rejects (?i-u:...) as Invalid group modifier. |
| `pi-coding-agent` | `example_permission_gate` | JS RegExp rejects (?i-u:...) as Invalid group modifier. |

## Process state and lifecycle (7 targets)

| Package | Original test target | Evidence / assignment note |
| --- | --- | --- |
| `pi-coding-agent` | `claude_bash_hook_extension` | Hook request pipe closes with EPIPE / Broken pipe. |
| `pi-coding-agent` | `detached_pyrun_runner_process` | No status-2 response before the existing timeout. |
| `pi-coding-agent` | `differential_claude_bash_hook` | Hook request pipe closes with EPIPE / Broken pipe. |
| `pi-coding-agent` | `host_relay_resident` | Spawn-failure fixture child exits unsuccessfully; underlying exit cause not established. |
| `pi-coding-agent` | `session_coordination_tools` | Supervisor does not become ready within 10000 ms. The test recursively spawns its own libtest executable with `supervisor` as a test filter; before/after orphan chains were stopped. Fixed2026-10-25: reuse the private bwrap KB/resident IPC/durable-request child helper, preserving exact Continue./sender/details assertions and adding source/native comparison. `/tmp/storage2-coordination-green.log`:16 PASS plus1 child helper invoked by the parent; no provider-backed process or recursive launch. |
| `pi-coding-agent-interactive` | `pi_coding_agent_interactive` | Two stalled async tests at timeout, plus four ordinary assertion failures; see names below. |
| `pi-js-compat` | `process_env` | Same-binary env test sees PI_PROCESS_ENV_NO_ALIAS from another case. |

## Observed parity mismatches; lane diagnosis required (27 targets)

| Package | Original test target | Evidence / assignment note |
| --- | --- | --- |
| `pi-ai` | `differential_github_copilot_oauth` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-ai` | `image_catalog_generator` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-ai` | `image_catalog_http` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-ai` | `image_catalog_prototype` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `bundled_extensions_context` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `bundled_extensions_tools` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `differential_args` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `differential_bundled_extensions` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `differential_compaction` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `differential_config_lossless_package` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `differential_interactive_components` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `differential_keybindings` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `differential_openai_remote_compact` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `differential_slash_commands` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `differential_supervisor_request_loop` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `differential_supervisor_service` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `differential_tree_selector` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `example_bash_spawn_hook` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `example_file_trigger` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `example_reload_runtime` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `example_subagent` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `experimental_cli` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `main_process` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `package_manager_cli` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-coding-agent` | `session_autoname` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-tui` | `raw_editor` | Existing assertion output differs from the pinned oracle or expected contract. |
| `pi-tui` | `verifier_regressions` | Existing assertion output differs from the pinned oracle or expected contract. |

## FLAKY: unchanged-case variability accepted by main

The composed after ledger contains 75 failed outcomes versus 76 before. These
nine outcome changes are explicitly classified **FLAKY** by main (2026-10-25),
not fixed or hidden by this layout change. Five cases change from failure to
pass; four change from pass to failure. All assertions remain unchanged. The
full comparison is saved in `/tmp/test-consolidate-proof/suite-comparison.json`.
Strict pass/fail-set equality was therefore **not** obtained; the unchanged-case
variability is an accepted verification limitation, not an all-green result.

| Package / original target | Test | Before → after | Status |
| --- | --- | --- | --- |
| `pi-ai / anthropic_oauth` | `manual_state_mismatch_fails_with_the_source_message` | pass → fail | FLAKY |
| `pi-coding-agent / detached_pyrun_runner_process` | `answers_status_requests_while_the_evaluation_runs` | fail → pass | FLAKY |
| `pi-coding-agent / differential_tree_selector` | `tree_filters_search_labels_and_entry_types_match_frozen` | fail → pass | FLAKY |
| `pi-coding-agent / differential_tree_selector` | `tree_terminal_height_numbers_match_frozen` | pass → fail | FLAKY |
| `pi-coding-agent / example_file_trigger` | `private_watch_injects_clears_ignores_blank_and_preserves_failure_match_ts` | fail → pass | FLAKY |
| `pi-coding-agent / host_relay_resident` | `spawn_failure_is_logged_like_frozen_without_executing_any_ssh` | fail → pass | FLAKY |
| `pi-coding-agent-interactive / lib` | `modes::interactive::interactive_mode::ui_tests::fork_selector_forks_from_the_selected_user_message` | pass → fail | FLAKY |
| `pi-coding-agent-interactive / lib` | `modes::interactive::interactive_mode::ui_tests::header_startup_and_registration_frames_match_frozen_expandability` | pass → fail | FLAKY |
| `pi-coding-agent-interactive / lib` | `modes::interactive::interactive_mode::ui_tests::tree_selector_navigates_to_the_selected_entry` | fail → pass | FLAKY |

## Additional introspection-only flake

The first final `--list` attempt ran the unchanged `harness = false`
`bash-runner-lifecycle` binary and failed its timing-sensitive case 19: actual
output was `ready`, expected output was `(no output)`. This aborts Cargo's
ordinary list traversal even though the target does not expose libtest names.
A bounded retry with `--no-fail-fast` completed successfully and listed all
6,364 entries. Both attempt logs are retained. This is separate from the nine
full-suite outcome differences above; no harness assertion was changed.

## Interactive successor repair (2026-10-25)

The interactive lane repaired the shared fixture stream collision, JavaScript/serde numeric snapshot mismatch and missing live header/resource/chat expansion. The two formerly stalled cases, four baseline assertions, fork flake, deterministic ownership regression and actual-source header expansion differential now pass together: 9/9 parallel, no ignored tests or timeout fixes. Source/cause details and exact revision-scoped proof are in [the active lane ledger](interactive-user-visible-wip.md#baseline-repair-checkpoint-2026-10-25). This supersedes the interactive blocker below only; other baseline failures remain open.

## Interactive timeout

The unchanged `pi-coding-agent-interactive --lib` binary stalled with these two tests still active:

- `modes::interactive::interactive_mode::ui_tests::hidden_coordination_queue_keeps_user_input_and_delivery_intact`
- `modes::interactive::interactive_mode::ui_tests::reload_while_streaming_matches_frozen_warning`

Both names appear in `/tmp/test-consolidate-proof/baseline-hang-stack-full.log`; both were parked in Tokio. The reload test retained its `interactive-reload-busy-oracle.mts` child. Parent approval permits a 600-second binary timeout for the after run as well. The baseline sandbox was terminated after the stall was diagnosed so `--no-fail-fast` could complete the other packages. This is an unchanged-library blocker, not a consolidation result.

## AI re-pin disposition (2026-10-25)

- `differential_github_copilot_oauth`: fixed the old-only individual policy fallback; unchanged pinned refresh/toAuth differential passes (2 tests).
- `differential_cloudflare_gateway_binding`, `differential_constrained_sampling`, `differential_deferred_tools`, `differential_model_catalog`: reproduced absent-source failures, then deleted the frozen-absent native modules, tests and oracles; no production consumers. Retired tests are not claimed as passing tests.
- `differential_text` and image catalog mismatches: still under diagnosis.

AI continuation: `differential_text` and native text helper retired with its absent frozen source; compaction consumers now inline exact frozen behavior (12 generation tests GREEN). Image generator/HTTP/prototype failures traced to a stale import of an auto-running script plus removed strict/parser contracts; whole-script oracle and native workflow re-ported, 5 tests GREEN, prototype proof retained through workflow. See [image trace](traceability/image-catalog-generator.md).
