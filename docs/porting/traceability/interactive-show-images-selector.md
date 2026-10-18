# show-images-selector.ts

Source: `coding-agent/src/modes/interactive/components/show-images-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/show_images_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| Yes/No items + descriptions | `ShowImagesSelectorComponent::new` | differential (`showImages`) |
| layout 12..32 primary width | `SHOW_IMAGES_SELECT_LIST_LAYOUT` | differential lines |
| preselect current value | `set_selected_index` | differential current `true` |
| select → `value === "yes"` | `on_select` | differential input `[B` + `
` |
| cancel | `on_cancel` | differential escape case (input sequence) |
| top/bottom borders | `dynamic_border` | differential lines |
| `getSelectList` | `select_list` / `select_list_mut` | differential drives the list |

Select-list styling uses `theme::get_select_list_theme()`, the ported `getSelectListTheme()` builder.
