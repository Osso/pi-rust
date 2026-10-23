# `components/assistant-message.ts`

Authority: complete 320-line [common-pin](../source-lock.json) module. Rust: `crates/pi-coding-agent/src/modes/interactive/components/assistant_message.rs`. [Contract](../../specs/assistant-narration.md) · [Narration helper](assistant-narration.md).

## Literal state representation

The source's per-index `ContentSlot` becomes a private struct with text/thinking/tool-call tag, optional typed Markdown/Text handle, optional trailing Spacer, previous component text, visible flag and hidden-thinking flag. The component retains its outer Container, typed content Container, leading Spacer, trailing status and last owned message. Markdown/Text objects survive unchanged updates instead of clearing/recreating their state.

| Export / branch | Native item | Proof |
|---|---|---|
| constructor defaults; inherited Container | `new` with five arguments; owned message, Rc MarkdownTheme, JsString label, existing native padding; outer Container delegation | existing default-theme source cases plus new empty-constructor/setter sequence; no old transformer argument |
| `invalidate`, hide/label/padding setters | invalidate children then reconcile cached message; setters reconcile immediately | full transition sequences, raw label/padding, repeated no-op setters and explicit invalidation |
| text slot, empty text and same Markdown reuse | `reconcile_markdown_slot` with text tag | text/empty/type transitions and observable highlighter-cache differential |
| thinking signature identifies public prose | shared `is_server_narration_signature`; same text-slot path | Base64/protobuf corpus against actual frozen component; hide-thinking never hides public narration |
| private hidden thinking | per-slot retained Text and styled native label | multiple nonempty thoughts separated by empty thought; two labels, not one coalesced run; raw labels |
| private visible thinking | retained Markdown with thinking color/italic style | hide/unhide/type transitions, default-theme source variants |
| thinking spacer iff later visible content | reverse slot scan; retained Spacer while needed | thinking/empty/tool/text sequences and whole rendered UTF-16 output |
| length / aborted / error / ordinary status | `status_message` plus retained `TrailingStatusSlot` | exact maximum-token notice, tool-call precedence, missing/empty/default-abort/raw errors; transitions remove obsolete status |
| source error-message truthiness | empty strings use Operation aborted / Unknown error; whitespace/raw text retained | status matrix compares actual source frames |
| leading spacer / children ordering | retained leading Spacer, rebuilt list of retained child handles | complete initial/update/setter frames, including invisible/empty/tool-only states |
| OSC133 A/B/C markers | native render wraps nonempty output only without tool calls | existing native marker tests and all differential frames |
| `updateContent(message)` API | no old `isStreaming` argument/field; no ignored transformer API | all native UI/resident-console/test callers migrated; source oracle uses frozen five-argument constructor and one-argument update |

## Observable retention proof

A custom highlighter returns numbered visible code text. The first source/native render matches `[1]cache`. Before the fix, an identical update rebuilt Markdown and rendered native `[2]cache` while the source retained `[1]cache`. The test also covers no-op padding, label changes, appended thinking, visibility changes and explicit invalidation. It asserts complete rendered output, not component identity or internal calls. This is observable cache lifetime, not an optional JS object-identity projection.

## Approved owned boundary and consumers

Last-message storage uses the general owned-value/event-snapshot approval; it does not make old event objects mutable. The core narration getter returns unchanged owned human prose and never mutates replay blocks.

A separate whole-mode consumer issue is explicitly retained in PLAN: agent-loop.ts:505/523 shallow-copy messages while sharing their content array; ignored private deltas can therefore update the source controller's old streamingMessage. toggleThinkingBlockVisibility calls setHide plus updateContent(streamingMessage). Native whole-mode re-pin must retain latest owned streaming snapshots for that action (and port its currently missing rebuild/propagation), without reintroducing mutable stream events. This component checkpoint does not claim that still-todo mode consumer is complete.

## Assertion and API reconciliation

Old tests expecting adjacent thoughts to coalesce into one hidden label and the old short truncation sentence were pre-repin contracts. Actual frozen RED proves the new per-slot count and exact maximum-output-token wording; assertions now require those literal outputs. Old ignored transformer/streaming parameters and the private mode getter were deleted, not retained as alternate paths. Existing rendering cases moved unchanged to a dedicated assistant test; the former ignored-transformer fixture remains the same mixed-case prose input without an obsolete API field.

## Proof ledger

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:

- `test-assistant-message-red-1.log` → `test-assistant-message-green-1.log`: 4 full frozen differentials (status/raw values, transition sequences, observable cache and signature corpus).
- `test-assistant-message-existing-diff-green.log`: 1/1 across 8 existing default-theme source variants.
- `test-assistant-message-native-green.log`: 5/5 marker/length/per-slot/padding tests.
- `test-assistant-message-raw-green.log`: 1/1 raw text/thinking/label/error.
- `test-assistant-narration-mode-green-1.log`: 1/1 actual hidden-thinking-end consumer ordering.
- `test-assistant-narration-print-green-1.log`: 1/1, two actual print-mode scenarios, real frozen/native stdout and provider replay types.

13 focused tests cover this coupled checkpoint. Final checkpoint formatting/all-target proof is recorded by SHA; parent owns final readability/clippy/independent gates. No new semantic deviation is claimed, and no whole interactive-mode/print-mode/Q13 acceptance is implied.
