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

## Q18 native default renderer (2026-10-22)

Removed encode_node_utf8 from the custom-type label and Markdown input. Native
style callbacks and render lines retain exact units through component output.
raw_leaf_components checks different raw label/content units in collapsed and
expanded default output. Written but unexecuted while coding compilation is
blocked; no common-pin whole-module parity claim.

### Re-pin gaps (frozen 4f9e6f8a4 custom-message.ts)

- Old-only output_pad: Rust retains MessageRenderOptions.output_pad and the custom
  message constructor/output-pad setter. Frozen custom-message.ts constructor
  lines26–31 and renderer options70–75 have no output_pad; frozen extension
  MessageRenderOptions likewise lacks it. Deletion touches shared types and204
  callers and is a separate post-compile RED-first commit.
- Renderer-error behavior: frozen custom-message.ts lines71–83 catches renderer
  throws and falls through to default rendering. Rust's callback contract/render
  path lacks that catch behavior. Separate post-compile RED-first slice.

Parent explicitly deferred these non-Q18 gaps; neither is an approved parity
deviation or new-pin proof.
