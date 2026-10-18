# coding-agent end-turn traceability

Pinned source: `packages/coding-agent/src/core/tools/end-turn.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/core/tools/end_turn.rs` (`EndTurnTool: AgentHarnessTool<ToolContext>`).

| Source | Rust | Tests |
|---|---|---|
| `endTurnSchema` (`reason` required) | `parameters()` | `tests/end_turn_tool.rs::schema_requires_a_reason` |
| blank-reason throw | `execute` (`pi_js_compat::string::js_trim`) | `rejects_a_blank_reason`, differential |
| `Turn ended: {reason}` + `{reason}` details + `terminate: true` | `terminating_text_result` | `terminates_with_its_required_reason`, differential |
| `promptGuidelines` | `PROMPT_GUIDELINES` | asserted by differential metadata consumers (registry not in this slice) |

Differential: `tests/differential_session_tools.rs::end_turn_tool_differential`.

## Gaps

- `renderCall`/`renderResult` are absent: the source tool has none.
