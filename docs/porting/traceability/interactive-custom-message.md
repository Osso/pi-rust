# components/custom-message.ts

Frozen source: `4f9e6f8a4`, `coding-agent/src/modes/interactive/components/custom-message.ts`.
Rust: `crates/pi-coding-agent/src/modes/interactive/components/custom_message.rs`.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| constructor; fixed default box padding 1/1 | `new` | native default rendering; differential text/blocks |
| `setExpanded`; invalidate rebuild | `set_expanded`; `invalidate` | native renderer expansion and throw/rebuild tests |
| custom renderer first, own styling | renderer arm | custom-renderer differential |
| undefined renderer result → default | fallthrough | default differential cases |
| renderer throw → default (:60–70) | `catch_unwind` around callback | native full-frame equality on construction, expansion, invalidation; throwing-renderer differential |
| `[customType]` label | `CustomMessageLabel` | default differential; raw leaf output |
| text content and text-block joining | `CustomMessageContent` arms | text/blocks differential |
| Markdown color | `CustomMessageText` callback | full ANSI differential |

## Common-pin API reconciliation (2026-10-22)

Frozen constructor (:20–24) has three arguments; renderer options contain only `expanded` (`core/extensions/types.ts:1331–1333`). Removed old-only `MessageRenderOptions.output_pad`, constructor padding argument, setter, interactive-mode propagation, and their padding-only fixture/oracle variants. Other components' output padding remains unchanged. Bundled message-renderer example uses frozen fixed `Box(1,1)` (`examples/extensions/message-renderer.ts:32–34`).

Frozen `rebuild` catches custom renderer exceptions and renders the default box. Rust now catches callback unwinding at the same boundary; rendering errors outside that callback are not caught. RED `test-custom-message-throw-red.log` reproduced the uncaught callback panic before implementation. No renderer exception exclusion remains for this component.

`differential_interactive_components::custom_messages_match_the_pinned_typescript_source` isolates all custom-message variants so unrelated component failures do not hide this module's results. Existing aggregate helper test no longer duplicates those cases.

## Lossless content

Label and Markdown input stay `JsString`; style callbacks and rendered lines retain exact UTF-16 units. `raw_leaf_components` checks different raw label/content units in collapsed and expanded default output. No scalar projection introduced.

## Proof ledger

Development artifacts: `/tmp/pi-rust-repin-lossless-interactive/`. RED above predates callback catch. GREEN commands and results are recorded in `REPORT.md` there; a compiled test is not treated as executed evidence.
