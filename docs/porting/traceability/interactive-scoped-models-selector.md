# scoped-models-selector.ts

Source: `coding-agent/src/modes/interactive/components/scoped-models-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/scoped_models_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `EnabledIds` (`null` = all) | `EnabledIds = Option<Vec<String>>` | differential + behavioral |
| `isEnabled` | `is_enabled` | row status marks |
| `toggle` (first toggle starts with one) | `toggle` | toggle test |
| `enableAll` (filtered target, returns null when complete) | `enable_all` | enable-all test |
| `clearAll` (all-enabled → explicit complement) | `clear_all` | clear-all test |
| `move` (bounds, swap) | `move_enabled` | reorder test |
| `getSortedIds` (enabled order then remaining) | `get_sorted_ids` | reorder/render tests |
| `ModelsConfig` | `ModelsConfig` | all tests |
| `ModelsCallbacks` | `ModelsCallbacks` (async → sync) | recorder tests |
| header/hint/search/list/footer layout | `new` + render | differential lines |
| `updateModels` (tri-state enabled, selection restore) | `update_models` | update test |
| `setRefreshStatus` | `set_refresh_status` | update test |
| `buildItems` | `build_items` | render |
| `getFooterText` (counts, unavailable, dirty) | `get_footer_text` | footer test + differential |
| `refresh` (fuzzy filter, clamp, footer) | `refresh` | search test |
| `updateList` (8-row window, marks, unavailable, model name) | `update_list` | unavailable test + differential |
| reorder up/down (enabled only, bounds) | `reorder` | 3217 reorder test |
| toggle confirm | `toggle_selected` | toggle test |
| enable-all / clear-all (filtered targets) | `enable_all_filtered` / `clear_all_filtered` | toggle test |
| toggle provider | `toggle_provider` | toggle test |
| save (persist, clear dirty) | `save` | 3217 save test |
| ctrl+c clears search then cancels | `handle_input` | escape/ctrl+c test |
| escape cancels | `handle_input` | escape test |
| search fallthrough | `handle_input` | search test |
| focus propagation | `Focusable` | code-mapped |

Adaptations: `onChange`/`onPersist` return `void | Promise<void>` in TS; the Rust callbacks are synchronous and callers spawn async work. `modelsById` uses `HashMap`; iteration order never escapes (all ordering comes from `all_ids`/`enabledIds`).

Gaps: the `\x1b[27;5;13~` save sequence (3217) is exercised with `ctrl+s`; the kitty sequence mapping belongs to `pi-tui::keys`. The 6949 `showModelsSelector`/InteractiveMode cases are outside this slice.
