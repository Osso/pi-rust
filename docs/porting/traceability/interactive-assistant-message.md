# components/assistant-message.ts

Source: `coding-agent/src/modes/interactive/components/assistant-message.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/assistant_message.rs`

Tests: `crates/pi-coding-agent/tests/interactive_components_c.rs` (assistant_*),
differential `assistantMessage` cases in `differential_interactive_components.rs`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `AssistantMessageComponent` | `AssistantMessageComponent` | behavioral + differential |
| `constructor(message?, hideThinkingBlock, markdownTheme, hiddenThinkingLabel, outputPad, transformers)` | `new(Option<AssistantMessage>, bool, Option<Rc<dyn MarkdownTheme>>, Option<String>, usize, Vec<MarkdownTransformer>)` | differential variants |
| `invalidate` rebuild from `lastMessage` | `Component::invalidate` | `user_message_reapplies_transformers_when_invalidated` pattern |
| `setHideThinkingBlock` | `set_hide_thinking_block` | hidden-thinking differential |
| `setHiddenThinkingLabel` | `set_hidden_thinking_label` | default label checked |
| `setOutputPad` | `set_output_pad` | `assistant_uses_configured_output_padding` |
| `render` OSC 133 A/B/C markers | `Component::render` | `assistant_adds_osc_zone_markers_without_tool_calls` |
| tool calls suppress zone markers | `has_tool_calls` guard | `assistant_skips_osc_markers_with_tool_calls` |
| text blocks trimmed and rendered as Markdown | `AssistantContent::Text` arm | differential text case |
| adjacent thinking blocks coalesce into one run | thinking loop | `assistant_coalesces_adjacent_thinking_blocks_into_one_label` |
| hidden thinking renders one label per run | `hide_thinking_block` arm | length/hidden differential |
| spacer only when visible content follows a thinking run | `has_visible_content_after` | differential thinking+text |
| `stopReason === "length"` truncation notice | `StopReason::Length` | `assistant_renders_length_stop_with_hidden_thinking_label` |
| aborted message text (`Request was aborted` → `Operation aborted`) | `StopReason::Aborted` arm | differential aborted |
| error message `Error: <message>` / `Unknown error` | `StopReason::Error` arm | differential error |
| Markdown transformer context (`messageType`, `isStreaming`, width) | `create_markdown_transform` | transformer/streaming/width tests |

## Gaps

- Transformers cannot return non-strings or throw in Rust; the source's try/catch chain
  continuation is not representable (typed API exclusion).
- `availableWidth` is passed as `usize`; the source's `number` cannot be fractional here.

## Discrepancies

None found.

## Q18 native UTF-16

Native style callbacks and render output; assistant text/thinking/labels/errors no longer encode before Markdown/Text. Behavioral cases in `raw_error_consumer_components` written **unexecuted** pending coding-agent compilation. Earendil image filesystem load omission follows frozen source lines 14-22; no change to that source behavior.
