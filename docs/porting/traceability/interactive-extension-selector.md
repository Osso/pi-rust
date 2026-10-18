# extension-selector.ts

Source: `coding-agent/src/modes/interactive/components/extension-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/extension_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| option rows, selected prefix | `update_list` | differential (`extension`) |
| up/down with `k`/`j` fallbacks | `handle_input` | differential `j` |
| `app.tools.expand` toggle | `on_toggle_tools_expanded` | differential `\t` event |
| confirm/cancel | `on_select` / `on_cancel` | differential |
| title text handle | `title_text` field | differential lines |
| timeout countdown (title `(Ns)`) | `CountdownTimer` + `poll` | behavioral `countdown_timer_*` covers the timer; component wiring is code-mapped |
| `dispose` | `dispose` | code-mapped |

Gap: the timeout wiring in the component (`timeout > 0 && tui` guard, title rewrite) has no direct test because it needs a live TUI request-render handle; the timer itself is behaviorally tested.
