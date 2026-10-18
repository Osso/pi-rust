# user-message-selector.ts

Source: `coding-agent/src/modes/interactive/components/user-message-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/user_message_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| initial selection from id, else newest | `UserMessageList::new` | differential `initialSelectedId: "b"` |
| empty list message | `UserMessageList::render` | `interactive_components::user_message_selector_auto_cancels_when_empty` |
| scroll window (`maxVisible 10`, half-window) | `start_index` / `end_index` | differential 3-message render (window covers all) |
| newline → space + trim | `js_trim(&text.replace('\n', " "))` | differential second message |
| cursor, bold selected, truncate `width - 2` | render loop | differential lines |
| metadata `Message N of M` | render loop | differential lines |
| scroll indicator | `start_index > 0 || end_index < len` | gap: needs >10 messages to observe |
| up/down wrap, confirm, cancel | `UserMessageList::handle_input` | differential up/confirm |
| header, borders, spacers | `UserMessageSelectorComponent::new` | differential lines |
| `setTimeout(onCancel, 100)` when empty | `poll(now)` | behavioral test |

Representation: the empty-list `setTimeout` is driven by `poll(now)`. Gap: the >10-message scroll indicator is not exercised (the source's window math is ported directly).
