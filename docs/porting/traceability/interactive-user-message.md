# components/user-message.ts

Source: `coding-agent/src/modes/interactive/components/user-message.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/user_message.rs`

Tests: `crates/pi-coding-agent/tests/interactive_components_c.rs` (user_message_*),
differential `userMessage` cases.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `UserMessageComponent` | `UserMessageComponent` | behavioral + differential |
| `constructor(text, markdownTheme, outputPad, transformers)` | `new(text, Option<Rc<dyn MarkdownTheme>>, usize, Vec<MarkdownTransformer>)` | differential |
| `setOutputPad` rebuild | `set_output_pad` | `user_message_uses_configured_output_padding` |
| background box (`userMessageBg`, padY=outputPad, padX=1) | `BoxComponent` | differential |
| Markdown color (`userMessageText`) | `DefaultTextStyle::color` | differential |
| options `preserveOrderedListMarkers`, `preserveBackslashEscapes` | `MarkdownOptions` | differential ordered-list case |
| transformer context `user`, `isStreaming=false` | `create_markdown_transform` | `user_message_chains_transformers_with_user_context` |
| OSC 133 A/B/C markers and stable height | `Component::render` | `user_message_keeps_height_and_zone_markers` |
| `invalidate` rebuild | `Component::invalidate` | `user_message_reapplies_transformers_when_invalidated` |

## Gaps

- Transformer exception continuation is not representable (same typed exclusion as assistant-message).

## Discrepancies

None found.

## Q18 native UTF-16

Markdown text, style callbacks, OSC composition and render lines retain `JsString`. `raw_error_consumer_components::user_message_keeps_raw_units_across_padding_and_invalidation` is written, **unexecuted** (coding-agent compiler-blocked). Re-pin gap: constructor ignores markdown transformers in current Rust (`_markdown_transformers`); frozen `user-message.ts` transformer creation and Markdown transform option remain unimplemented, not parity evidence.
