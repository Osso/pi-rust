# config-selector.ts

Source: `coding-agent/src/modes/interactive/components/config-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/config_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `ResourceType` | `ResourceType` | grouping/render tests |
| `formatBaseDir` | `format_base_dir` | group labels in differential renders |
| `getGroupLabel` | `get_group_label` | differential renders (`npm:pi-tools (user) · inherited global`) |
| `buildGroups` | `build_groups` | differential renders; group order by origin/scope/source |
| `FlatEntry` | `FlatEntry` (indices instead of object refs) | navigation/search tests |
| `ConfigSelectorHeader` render | `ConfigSelectorHeader::render_lines` | scope switch test + differential renders |
| `ResourceList` construction/chrome | `ResourceList::new` (`chrome = 8`, min 5) | render tests |
| `setWriteScope` | `set_write_scope` | tab switch test |
| `groups` getter | `scope_index` / `groups` | tab switch test |
| `buildInheritedEnabledMap` | `build_inherited_enabled_map` | inherited cycle test |
| `buildFlatList` | `build_flat_list` | search/navigation tests |
| `findNextItem` | `find_next_item` | navigation via keybindings (gap: no direct test) |
| `filterItems` | `filter_items` | search test (`baz`, `zzz` no-match) |
| `selectFirstItem` | `select_first_item` | search test |
| `updateItem` | `update_item` | project package cycle test |
| `render` (search, no-results, group/subgroup/item rows, cursor, scroll info) | `render_lines` | differential renders; inherited dimming test |
| `handleInput` navigation/page/cancel/ctrl+c/tab | `handle_key_input` | tab tests; page/cancel via keybindings (gap: no direct test) |
| confirm/space branch + user-scope guard | `handle_key_input` | project cycle, global top-level, package tests |
| search fallthrough | `handle_key_input` | search test |
| `toggleResource` | `toggle_resource` | all cycle tests |
| `toggleTopLevelResource` | `toggle_top_level_resource` | global top-level test |
| `togglePackageResource` | `toggle_package_resource` | project package cycle; global package path shares helpers (gap: no dedicated global package test) |
| `renderCheckbox` | `render_checkbox` | differential renders (`[-]`, `[+]`, `[x]`) |
| `getItemSuffix` | `get_item_suffix` | differential renders (`project load`, `project unload`, `inherited global`) |
| `isDimmedItem` | `is_dimmed_item` | inherited dimming test |
| `setProjectResourceOverride` | `set_project_resource_override` | both project cycle tests |
| `setProjectTopLevelOverride` / `setProjectTopLevelPaths` | `set_project_top_level_override` / `set_top_level_paths` | inherited top-level cycle test |
| `setProjectPackageOverride` | `set_project_package_override` | project package cycle test |
| `getNextOverrideState` | `get_next_override_state` | cycle tests |
| `getProjectOverrideState` | `get_project_override_state` | cycle tests |
| `getOverrideStateFromEntries` | `get_override_state_from_entries` | cycle tests |
| `getInheritedEnabled` | `get_inherited_enabled` | cycle tests |
| `isInheritedGlobalItem` | `is_inherited_global_item` | dimming/cycle tests |
| `getTopLevelOverridePatterns` | `get_top_level_override_patterns` | inherited top-level cycle test |
| `getResourcePatternForScope` | `get_resource_pattern_for_scope` | top-level override tests |
| `createPackageOverrideSource` | `create_package_override_source` | project package cycle (`autoload:false`) |
| `packageSourceStringMatches` | `package_sources_match` | package cycle |
| `findMatchingPackageSource` | `find_matching_package_source` | project override state |
| `getPatternEntryTarget` | `pattern_entry_target` | cycle tests |
| `getResourceItemKey` | `resource_item_key` | inherited map |
| `getItemScope` | `get_item_scope` | user-scope guard |
| `getTopLevelBaseDir` | `get_top_level_base_dir` / `get_top_level_base_dir_str` | override patterns |
| `getResourcePattern` / `getPackageResourcePattern` | same names | toggle tests |
| `ConfigSelectorComponent` constructor/switch/getResourceList | `new(ConfigSelectorOptions)` / `switch_write_scope` / `get_resource_list(_mut)` | tab tests, differential |
| component focus propagation | `Focusable` | code-mapped |

Adaptations:
- `ResourceList` stores `FlatEntry` indices instead of `ResourceItem`/`ResourceGroup` references; identical ordering and identity because the lists are rebuilt only on scope changes.
- The TypeScript component leaves tab handling to `ResourceList.onSwitchMode`. Rust records `switch_requested` in `ResourceList` and the owning component performs the scope switch plus `request_render` after `handle_input`, preserving the `tui.input.tab` keybinding match (including remaps) without a self-referential callback.
- Project settings setters return `Result`; failures are discarded because the component has no error channel (TypeScript would throw out of `handleInput`). Gap: no write-failure test.

Gaps: page up/down, cancel, and ctrl+c branches have no direct behavioral test; they are exercised only through keybinding matching.
