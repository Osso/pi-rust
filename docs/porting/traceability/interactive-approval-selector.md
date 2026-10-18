# approval-selector.ts

Source: `coding-agent/src/modes/interactive/components/approval-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/approval_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `APPROVAL_PRESETS` rows | `update_list` | differential (`approval`) |
| current preset checkmark | `is_current` | differential (`ask-me`) |
| scope toggle global/project | `handle_input` `\t` branch | differential event scope `project` |
| up/down with `k`/`j` fallbacks | `handle_input` | differential `j` |
| confirm emits `{preset, scope}` | `ApprovalSelectorSelection` | differential event |
| cancel | `on_cancel` | covered by escape sequence |
| static layout + key hints | `new` | differential lines |

No gaps.
