# theme-selector.ts

Source: `coding-agent/src/modes/interactive/components/theme-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/theme_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `getAvailableThemes()` | `themes: &[String]` constructor parameter | differential probes `getAvailableThemes()` from the oracle and feeds the same list |
| `(current)` description | `SelectItem::with_description` | differential current theme |
| preselect current theme when found | `set_selected_index` | differential |
| `onSelect` / `onCancel` / `onSelectionChange` | `select_list.on_select` / `on_cancel` / `on_selection_change` | differential events |
| max visible 10, layout 12..32 | constructor constants | differential lines |
| borders + `getSelectList` | `dynamic_border`, `select_list_mut` | differential |

Adaptation: the theme registry lives in the interactive theme slice, so the available names are injected. The differential test asks the TypeScript module for the real list to keep both sides identical.
