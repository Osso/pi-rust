# components/select-list.ts traceability

## Q2 frozen revalidation

Frozen select-list.ts (229 lines) audited whole. Exports SelectItem/theme/layout/truncator context and constructor/filter/selected-index/invalidate/render/input/getSelectedItem map to select_list.rs. Primary bounds, label/value choice, descriptions, scroll info and callbacks have native select_list and differential_components proof. Empty JS-trimmed descriptions do not create columns. Custom truncator maxWidth/columnWidth are **i64**, preserving negative internally-derived widths: render0..4 yields -4..0 before output truncation. Native/frozen differential repin_components proves callback values and output. Signed-width/JS-trim/empty-description mutants are killed. Typed setter and UTF8/String boundaries remain explicit; no unsigned callback deviation remains. [Q2 proof](../q2-tui-components-proof.md).


Source: `packages/tui/src/components/select-list.ts` (229 lines). Rust:
`crates/pi-tui/src/components/select_list.rs`.

Tests: `crates/pi-tui/tests/select_list.rs` (every listed TS test ported plus
interactive cases), `crates/pi-tui/tests/differential_components.rs::select_list_render_and_callbacks_match_typescript`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `interface SelectItem` | `struct SelectItem` (+ `new`, `with_description`) | all |
| `interface SelectListTheme` | `struct SelectListTheme` (+ `plain()`) | all |
| `interface SelectListTruncatePrimaryContext` | `SelectListTruncatePrimaryContext` | truncation-override test, differential |
| `interface SelectListLayoutOptions` | `SelectListLayoutOptions` | min/max/override tests |
| `class SelectList` | `struct SelectList` | all |
| `constructor(items, maxVisible, theme, layout = {})` | `SelectList::new` | all |
| `setFilter` | `set_filter` | `filtering_resets_selection_and_reports_no_match`, differential |
| `setSelectedIndex` | `set_selected_index` | `filtering_resets_selection...`, differential |
| `invalidate` | `Component::invalidate` (no-op) | source has no cached state |
| `render(width)` | `Component::render` | all render tests, differential |
| `handleInput` | `Component::handle_input` | wrap/confirm/cancel tests, differential |
| `getSelectedItem` | `get_selected_item` | filtering test, differential |
| `onSelect` / `onCancel` / `onSelectionChange` | public `Option<Rc<dyn Fn>>` fields | callback tests, differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| empty filtered list → no-match line | `filtered_items.is_empty()` | `filtering_resets_selection_and_reports_no_match` |
| scroll window `selected - floor(maxVisible/2)` clamped | `saturating_sub`/`min` | `scrolls_window_and_renders_scroll_indicator` |
| scroll indicator when start > 0 or end < len | same | scroll test, differential |
| description path requires `width > 40` | `width > 40` | differential width 41/80 |
| effective primary column `max(1, min(primary, width - prefix - 4))` | same | alignment tests |
| remaining description width `> 10` | `remaining_width > MIN_DESCRIPTION_WIDTH` | differential width 41 |
| selected line styled as one unit | `selected_text` over prefix+value+spacing+desc | marker differential |
| unselected line styles only description | `description` over spacing+desc | differential |
| no-description path `width - prefix - 2` | same | scroll test |
| custom `truncatePrimary` then re-truncate | `truncate_primary` | override test, differential |
| primary column width from widest item clamped to bounds | `get_primary_column_width` | min/max tests |
| min/max bound derivation (`rawMin/rawMax` swap) | `get_primary_column_bounds` | min/max tests |
| label fallback to value | `get_display_value` | `uses_label_fallback_value_and_selected_theme` |
| up/down wrap and notify | `handle_input` | wrap test, differential |
| confirm/cancel callbacks | `handle_input` | confirm/cancel test, differential |
| multiline description normalization | `normalize_to_single_line` | normalization test, differential |

## Behavioral notes

- Empty-list up/down: the source sets `selectedIndex = -1` transiently; Rust
  keeps index 0 because `usize` cannot be negative. No observable difference:
  no item, no callback, no render path uses the index while the list is empty.
- `String.prototype.trim()` is reproduced with Rust `trim()`; the differential
  descriptions use ASCII whitespace only.

## Gaps / exclusions

- JavaScript callers passing fractional/negative indices or non-string fields
  are excluded by the typed Rust API (decision 2026-08-12). `set_selected_index`
  takes `isize` and clamps like the source.

## Discrepancies

None found.
