# Rejectable stream acquisition

Reference: common-pin `agent-core/src/types.ts` StreamFn; `agent-loop.ts` stream acquisition; `harness/agent-harness.ts:555–605`; coding `compaction/compaction.ts:697–708`, `compaction/branch-summarization.ts:349–353`.

## Contract and implementation

`StreamFn` returns `Result<AssistantMessageEventStream, AgentError>`. Low-level acquisition rejection escapes before assistant message events; the abort race still bounds acquisition. Provider failures encoded as assistant completions remain distinct from rejection of acquisition or `stream.result()`.

Coding summarization now awaits acquisition and result directly, preserving rejection text without a fabricated assistant error or added prefix. Retired old-only summary retries, retry options/callback arguments, no-op callback type, callback-less alternate wrapper and generated request session IDs. Keep frozen `cacheRetention: none`; do not inject session IDs. SDK invalid timeout settings reject acquisition rather than creating a failed stream. Agent-session compaction/tree callers propagate failure and clear the branch-summary abort controller.

This bounded correction leaves existing coding summary-usage fields unchanged; it is not a whole coding-compaction module parity claim.

## Proof matrix

| Boundary | Proof |
|---|---|
| Before stream exists; native raw error; no fabricated assistant/turn-end/agent-end | `agent_loop::rejects_stream_acquisition_without_fabricating_an_assistant_message`, RED `stream-acquisition-red.log`, GREEN `stream-acquisition-green.log` |
| Frozen steering/follow-up/acquisition rejection event ordering | `differential_agent_loop::queue_and_stream_rejections_match_the_pinned_typescript_source`; 3 scenarios plus 35 retained scenarios |
| Coding summary, native-checkpoint materialization and branch summary acquisition rejection | `summary_stream_acquisition`, 2 tests /3 API paths; unchanged raw error and one attempt |
| Rejected stream result versus provider error completion | `coding_compaction_propagates_failed_stream_message_without_adding_a_prefix`; existing provider-stop-error tests retain their source prefix |
| No retry or generated request session ID | `compaction_generation` (12 tests), request shape +one-attempt transient error and checkpoint materialization assertions |
| SDK and agent-session consumers | 11 SDK +9 compaction-extension +14 branch-summary tests; real settings rejection and real successful model paths |

Artifacts `/tmp/pi-rust-repin-delta-agent-core/`: `stream-core-green.log` (31 Agent +43 loop +2 grouped differentials +3 stream-option records +10 native provider-error tests), `summary-acquisition-green2.log`, `stream-coding-development.log`, `stream-coding-green.log`, `stream-workspace-callers2.jsonl` (core+coding all-targets compilation: zero errors/warnings; later test-only edits compiled by focused tests).

Working harness must still catch rejected acquisition and emit its own exact failure content `[{ type: "text", text: "" }]`. This is not yet implemented; the low-level migration removes the prior inability to express that boundary.
