# thinking-selector.ts

Source: `coding-agent/src/modes/interactive/components/thinking-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/thinking_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `LEVEL_DESCRIPTIONS` for all 8 levels | `level_description` | differential renders `off`, `low`, `high`; remaining variants are the same exhaustive match |
| items from `availableLevels` | `ThinkingSelectorComponent::new` | differential |
| max visible = item count | `SelectList::new(items, items.len(), ..)` | differential |
| preselect current level when present | `set_selected_index` | differential `low` |
| select → level | `on_select` | differential `[B` + `
` |
| cancel | `on_cancel` | covered by escape input sequence |
| layout 12..32 | `THINKING_SELECT_LIST_LAYOUT` | differential lines |

Gap: the oracle covers three levels; the other description strings are asserted only by the exhaustive match (no separate render case).
