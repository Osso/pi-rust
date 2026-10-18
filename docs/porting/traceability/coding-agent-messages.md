# coding-agent messages traceability

Pinned source: `packages/coding-agent/src/core/messages.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust module: `crates/pi-coding-agent/src/core/messages.rs`.
Focused tests: `crates/pi-coding-agent/tests/messages.rs` (2).

The coding-agent module duplicates the agent harness message module except for `isCutPointMessage`. The Rust port re-exports `pi_agent_core::harness::messages` instead of duplicating the constants, records, constructors, and `convertToLlm`; that module's differential proof (`serialize_conversation`) covers the conversion behavior.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `COMPACTION_SUMMARY_PREFIX/SUFFIX`, `BRANCH_SUMMARY_PREFIX/SUFFIX` | exact framing strings | re-exported from `pi_agent_core::harness::messages`; agent-core marker proof |
| `BashExecutionMessage`, `CustomMessage`, `BranchSummaryMessage`, `CompactionSummaryMessage` | custom message records | `BashExecutionMessage` / `BranchSummaryMessage` re-exports; custom messages are `AgentMessage::Custom` JSON objects per the harness contract |
| `isCutPointMessage` | user and assistant only | `is_cut_point_message`; `accepts_conversational_boundaries_and_rejects_continuation_messages` |
| `bashExecutionToText` | command/output/exit/cancel/truncation text | re-exported `bash_execution_to_text`; agent-core tests |
| `createBranchSummaryMessage` / `createCompactionSummaryMessage` | timestamp-ms custom messages | re-exported constructors; session-manager context tests |
| `createCustomMessage` | custom message with content/details/display | re-exported `create_custom_message`; `converts_extension_custom_messages_to_user_text` |
| `convertToLlm` | bash/custom/branch/compaction mapping, excluded bash, pass-through roles | re-exported `convert_to_llm`; `converts_extension_custom_messages_to_user_text` plus agent-core conversion proof |

## Gaps

- `CustomMessage` is represented by `AgentMessage::Custom` rather than a distinct struct, matching the agent-core typed boundary.
- Timestamp strings parse through `iso_time::parse_javascript_date`; invalid timestamps map to `0` rather than a `NaN` timestamp.

## Discrepancies

None.
