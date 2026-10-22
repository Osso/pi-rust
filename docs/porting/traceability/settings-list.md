# components/settings-list.ts traceability

## Q2 frozen revalidation

Frozen settings-list.ts (250 lines) audited whole. SettingItem/SettingsListTheme/options; constructor/updateValue/invalidate/render; scrolling/alignment/description/hints; navigation/cancel; values cycling/submenu callback/selection restoration/filter map to settings_list.rs. A single Space always activates, including during search. Multi-character search input strips spaces. `differential_settings_list` compares render/callback/submenu/search matrix against frozen; native settings_list tests prove both Space and sanitized query behavior. Coding-agent row-order fixture explicitly enables Kitty capability, and search fixtures strip spaces. Space/sanitization mutants are killed. [Q2 proof](../q2-tui-components-proof.md).


Source: `packages/tui/src/components/settings-list.ts` (249 lines). Rust:
`crates/pi-tui/src/components/settings_list.rs`.

Tests: `crates/pi-tui/tests/settings_list.rs` (both listed TS tests ported),
`crates/pi-tui/tests/differential_settings_list.rs` (six scenarios: search,
space shortcut, navigation/cycling/description/scroll indicator, submenu,
no-match, empty list).

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `interface SettingItem` | `SettingItem` | all |
| `interface SettingsListTheme` | `SettingsListTheme` (+ `plain`) | all |
| `interface SettingsListOptions` | `SettingsListOptions { enable_search }` | search tests |
| `class SettingsList` | `SettingsList` | all |
| `constructor(items, maxVisible, theme, onChange, onCancel, options?)` | `SettingsList::new` | all |
| `updateValue` | `update_value` | differential |
| `invalidate` | `Component::invalidate` (delegates to submenu) | not directly asserted |
| `render` | `Component::render` | all |
| `handleInput` | `Component::handle_input` | all |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| submenu active → render/input delegated | `render`, `handle_key_input` | submenu differential |
| search input render + blank line | `render_main_list` | search differential |
| empty items → hint (+ hint line when searching) | early return | empty-list differential |
| filtered empty → no-match hint + hint line | early return | no-match differential |
| scroll window and indicator | `start_index`/`end_index` | navigation differential |
| max label width `min(30, max(...))` over all items | same | marker differential |
| value truncation with `""` ellipsis | `value_max_width` | differential |
| item line truncation with default ellipsis | `truncate_to_width` | marker differential |
| description wrapped at `width - 4` and styled | `wrap_text_with_ansi` | description differential |
| up/down wrap over display items | `handle_key_input` | navigation differential |
| confirm/space activation; space requires empty search | activation guard | TS tests + differential |
| cancel callback | `on_cancel` | submenu/no-match differential |
| search input updates + fuzzy filter | `apply_filter` | search differential |
| value cycling (`indexOf` -1 → first) | `activate_item` | TS tests + differential |
| submenu open, `done(value)` updates item + closes, `done()` closes | `activate_item`, `close_submenu` | submenu differential |

## Behavioral notes

- Mutable item/submenu state lives behind `Rc<RefCell<SettingsListState>>` so the
  submenu `done` callback can update items later without borrowing the component.
- `onChange`/`onCancel` are `Rc<dyn Fn>`; the TypeScript optional callbacks are
  required in Rust because the constructor always receives them.
- Search uses `fuzzy_filter` over item indices, preserving the source ordering.

## Gaps / exclusions

- Submenus are `ComponentRef`s; the source's `submenu` callback returning a
  component is typed the same way.
- No listed test covers `invalidate`; it delegates to the active submenu.

## Discrepancies

None found. The differential caught an ellipsis mismatch in hint/item
truncation during implementation; fixed to use the default `"..."` like the
source.

## Q2 findings correction

Empty selected descriptions skip the description block; whitespace-only descriptions remain truthy. `empty_description_skips_block_but_whitespace_does_not` compares absent/empty/space/nonempty rendered callback markers to frozen TS.
