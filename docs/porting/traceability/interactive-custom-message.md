# components/custom-message.ts

Source: `coding-agent/src/modes/interactive/components/custom-message.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/custom_message.rs`

Tests: `interactive_components_c.rs::custom_message_*`, differential `customMessage`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `CustomMessageComponent` | `CustomMessageComponent` | behavioral + differential |
| `setExpanded` | `set_expanded` | differential |
| `setOutputPad` | `set_output_pad` | padding test + differential |
| custom renderer first, own styling | `custom_renderer` arm | differential `pad`/`expanded` |
| renderer returns `undefined` -> default | fallthrough | differential default cases |
| default box (`customMessageBg`, pad 1/1) | `BoxComponent` | differential |
| `[customType]` label | `ThemeColor::CustomMessageLabel` | differential |
| string content | `CustomMessageContent::Text` | differential |
| block content joins text blocks with `\n` | `Blocks` arm | differential blocks case |
| Markdown color `customMessageText` | `DefaultTextStyle::color` | differential |

## Gaps

- Renderer exceptions cannot be represented (same typed exclusion as custom-entry).

## Discrepancies

None found.
