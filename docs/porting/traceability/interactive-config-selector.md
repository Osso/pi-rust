# config-selector.ts

Frozen: `4f9e6f8a4`, `coding-agent/src/modes/interactive/components/config-selector.ts`.
Native: `crates/pi-coding-agent-ui/src/modes/interactive/components/config_selector.rs`.

| Symbol / branch | Native | Behavioral proof |
|---|---|---|
| `ResourceType`, `formatBaseDir`, `getGroupLabel`, `buildGroups` | resource enums/group builder | differential mixed package/user/project/top-level resources, explicit/implicit bases and all four categories |
| `FlatEntry`, `buildFlatList`, `selectFirstItem`, `findNextItem`, `filterItems` | indexed groups and entries | differential initial frames, arrows, paging, matching/no-match search, empty list |
| `ConfigSelectorHeader` | fixed Resource Configuration title and space-toggle/esc-close hint | exact ANSI frames, narrow widths |
| `ResourceList` constructor/focus/chrome | flat `ResolvedPaths`, min-five visible rows, native input | differential heights 8/10/13/24, native raw focused query |
| `updateItem`, render checkbox/name/group/scroll | binary enabled state | differential frames after toggles and scrolling |
| `handleInput` | source navigation/cancel/exit/confirm and search fallthrough; no scope-switch branch | differential event sequence; tab/native header test |
| `toggleResource`, `toggleTopLevelResource` | resource metadata selects settings scope; exact relative +/- patterns | differential categories/scopes, native project exact-pattern replacement |
| `togglePackageResource` | string-to-filter conversion; exact source match; preserved other filters including empty arrays | differential three toggles, empty other filter, absent package; native global-package persistence |
| `getTopLevelBaseDir`, `getResourcePattern`, `getPackageResourcePattern` | native lexical relative paths | differential explicit/implicit bases, native raw pattern persistence |
| `ConfigSelectorComponent` constructor and `getResourceList` | flat options; forwards focus/input to owned resource list | complete frames/events plus CLI terminal probes |

Removed old-only global/project views, scope switching, inherited-resource dimming, three-state overrides and their APIs/tests. Source writes to the resource's own metadata scope, not a selected write scope. Native fields, search, names, patterns and frames retain `JsString` units; `raw_resource_name_query_and_persisted_pattern_keep_exact_units` asserts unmatched-surrogate preservation.

## Proof ledger

- RED: `/tmp/pi-rust-repin-lossless-interactive/test-config-selector-red.log` — original selector fails frozen frames (Project Local Resources vs Resource Configuration).
- GREEN: `test-config-selector-green-3.log` — dedicated differential 1/1 across six scenarios, exact complete frames/settings/events; includes tabs, cancel, default ctrl+c cancellation, all resource kinds/scopes, absent package and empty results.
- Post-commit `13e744ba`: `test-config_selector-flat-config-native-2.log`4/4; `test-config_selector_cli-flat-config-6714.log`4/4 (+1 ignored probe); `test-package_command_execution-flat-config-6714.log`31/31; `test-package_manager_cli-flat-config-6714.log`8/8. Parent owns final checks/readability/independent gate.

## Remaining proof boundaries

- No claim for duplicate path/type entries across groups, the frozen non-total comparator between project/temporary scopes, or filesystem settings-write failure parity. These are not covered by the six scenarios.
- Native CLI settings-manager identity is documented in [config-selector](config-selector.md); no whole-package-command parity claim.
