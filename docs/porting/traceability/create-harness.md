# coding-agent create-harness traceability

Pinned source: `packages/coding-agent/src/server/create-harness.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/server/create_harness.rs`.
Focused tests: `crates/pi-coding-agent/tests/create_harness.rs` (2).

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `CodingAgentHarnessTool` | harness tool + `promptSnippet`/`promptGuidelines` | `CodingAgentHarnessTool` |
| `buildCodingAgentHarnessSystemPrompt` | active-tool lookup, snippet whitespace collapse/trim, snippet map, guideline flattening, base options override | `build_coding_agent_harness_system_prompt`; `active_tools_contribute_collapsed_snippets_and_guidelines_in_order`, `inactive_tools_are_excluded_and_blank_snippets_are_skipped` |
| `createCodingAgentHarnessTool` | constrained sampling + execute context forwarding | Rust harness tools bind their context at construction; no forwarding wrapper is needed |
| `createCodingAgentHarness` | default read/bash/edit/write tools and system-prompt factory | **blocked** on the `bashToolSystemPromptContribution` and friends from `core/tools/*` (`ca-tools-core`) |

## Gaps

- The default-tool factory is blocked on the tool prompt contributions owned by `ca-tools-core`. The provided-tool
  prompt assembly is complete.

## Discrepancies

None.
