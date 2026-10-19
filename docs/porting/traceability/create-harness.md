# coding-agent create-harness traceability

Pinned source: `packages/coding-agent/src/server/create-harness.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/server/create_harness.rs`.
Focused tests: `crates/pi-coding-agent/tests/create_harness.rs` (5).

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `CodingAgentHarnessTool` | harness tool + `promptSnippet`/`promptGuidelines` | `CodingAgentHarnessTool`; the prompt metadata lives on `pi_agent_core::harness::agent_harness::HarnessTool` so it survives `setTools` |
| `buildCodingAgentHarnessSystemPrompt` | active-tool lookup, snippet whitespace collapse/trim, snippet map, guideline flattening, base options override | `build_coding_agent_harness_system_prompt`; `active_tools_contribute_collapsed_snippets_and_guidelines_in_order`, `inactive_tools_are_excluded_and_blank_snippets_are_skipped` |
| `createCodingAgentHarnessTool` | constrained sampling + execute context forwarding | `ConstrainedSamplingTool` adds `getExperimentalToolSampling`; Rust harness tools bind their context at construction, so no forwarding wrapper is needed |
| `createCodingAgentHarness` | default read/bash/edit/write tools and system-prompt factory | `create_coding_agent_harness`; `default_tools_and_explicit_harness_options_are_applied`, `caller_supplied_tools_and_activation_are_preserved`, `bash_receives_the_session_file_and_current_model_state` |
| Bash `prepare` | `PI_SESSION_ID`, `PI_SESSION_FILE`, `PI_PROVIDER`, `PI_MODEL`, `PI_REASONING_LEVEL` from live harness state | `HarnessBashPrepare`; `bash_receives_the_session_file_and_current_model_state` (session file + model/thinking change) |
| Dynamic system prompt | rebuild from current harness tools/active names | `dynamic_system_prompt` provider; assembly covered by `build_coding_agent_harness_system_prompt` tests |

## Gaps

- `AgentHarness::create` stores the `system_prompt` provider but the pi-agent-core scaffold does not yet invoke it;
  the provider and its dynamic lookup are wired here.
- The `getHarness()` "before initialization" error is unreachable through the public API; bash `prepare` returns the
  same message and the system-prompt provider panics on the unreachable path.

## Discrepancies

None.
