# components/compaction-summary-message.ts

Source: `coding-agent/src/modes/interactive/components/compaction-summary-message.ts`
Rust: `crates/pi-coding-agent-ui/src/modes/interactive/components/compaction_summary_message.rs`
(+ `core::messages::CompactionSummaryMessage` value record)

Tests: `interactive_components_c.rs::compaction_summary_*`,
differential `compactionSummary` cases.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `CompactionSummaryMessageComponent` | `CompactionSummaryMessageComponent` | behavioral + differential |
| `setExpanded` | `set_expanded` | differential |
| `formatCompactionTokenSummary` | `format_compaction_token_summary` | differential |
| tokens after + kept suffix | `tokens_after` / `kept_from_previous_context_tokens` | `compaction_summary_shows_after_kept_and_remote_result` |
| remote result suffix | `compacted_result_tokens` | differential |
| `formatTokenCount` (`<1000` decimal, else `toFixed(1)k`) | `format_token_count` + `pi_js_compat::number::js_to_fixed` | differential |
| `formatCompactionDuration` (`ms` / `toFixed(1)s`) | `format_compaction_duration` | differential |
| collapsed hint with `app.tools.expand` | `key_text` | differential |

## Gaps

- `CompactionSummaryMessage` is defined in `core/messages.rs`; the agent-core module
  keeps compaction messages as JSON objects, so no agent-core record exists to reuse.

## Discrepancies

None found.

## Q18 native UTF-16

Native render/style callbacks and summary composition; `raw_error_consumer_components::compaction_expanded_summary_keeps_raw_units` passes (verified 2026-10-26 at `11aa7a810`, `suite_core_4`, 19/19 in that module); the shared core message summary is native. No internal encoding.
