# keybindings.ts traceability

Source: `packages/tui/src/keybindings.ts` (325 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-tui/src/keybindings.rs`.

## Exported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `Keybindings` (declaration-merged interface) | `tui_keybindings()` returns the ordered table; downstream crates add ids by extending the `Vec` passed to `KeybindingsManager::new` | `binds_*`, `resolved_bindings_include_every_definition_in_order` |
| `Keybinding` (`keyof Keybindings`) | `&str` id | all tests |
| `KeybindingDefinition` | `KeybindingDefinition` | `ignores_user_bindings_for_unknown_ids_and_resolves_definitions` |
| `KeybindingDefinitions` | `Vec<(String, KeybindingDefinition)>` | `tui_keybindings()` |
| `KeybindingsConfig` | `Vec<(String, Vec<KeyId>)>` (ordered pairs; TS object key order is observable through conflicts) | `reports_direct_user_binding_conflicts_without_evicting_defaults`, `set_user_bindings_rebuilds_keys_and_conflicts` |
| `TUI_KEYBINDINGS` | `tui_keybindings()` | every keybinding test |
| `KeybindingConflict` | `KeybindingConflict` (`PartialEq`) | conflict tests |
| `KeybindingsManager` | `KeybindingsManager` | all tests |
| `setKeybindings` | `set_keybindings` | global identity test |
| `getKeybindings` | `get_keybindings` | global identity test |

## Branches

| Source branch | Rust item | Tests |
| --- | --- | --- |
| `normalizeKeys(undefined)` → `[]` | `normalize_keys(None)` | `leaves_dedicated_prompt_history_navigation_unbound_by_default` |
| single key vs array | `Vec` input only (typed core excludes the `KeyId` union) | `binds_ctrl_r_*` |
| dedupe keys preserving first occurrence | `normalize_keys` | `user_bindings_replace_defaults_and_deduplicate_keys` |
| unknown user binding id skipped | `rebuild` definition check | `ignores_user_bindings_for_unknown_ids_and_resolves_definitions` |
| user claims dedupe per id and record conflicts only for >1 claimant | `rebuild` user-claim list | conflict tests |
| conflict claimant order follows config insertion order | ordered `Vec` claims | `set_user_bindings_rebuilds_keys_and_conflicts` |
| default keys used when user keys undefined; user keys (even empty) replace defaults | `rebuild` match | `does_not_evict_selector_confirm_when_input_submit_is_rebound`, `user_bindings_replace_defaults_and_deduplicate_keys` |
| `matches` short-circuits on first matching key | `matches` | `binds_ctrl_j_as_default_newline_alias` |
| `getKeys` returns a copy | `get_keys` | all |
| `getDefinition` returns the table entry | `get_definition` | definition test |
| `getConflicts` deep-copies | `get_conflicts` | conflict tests |
| `setUserBindings` rebuilds | `set_user_bindings` | `set_user_bindings_rebuilds_keys_and_conflicts` |
| `getUserBindings` shallow-copies | `get_user_bindings` | `ignores_user_bindings_*` |
| `getResolvedBindings` single key → scalar, else array | `get_resolved_bindings` returns `Vec` for both cases (typed core excludes the union) | `resolved_bindings_include_every_definition_in_order` |
| lazy global manager creation | `get_keybindings` via `OnceLock<Mutex<Option<_>>>` | `global_registry_holders_share_mutation_and_replacement_identity` |

## Differential

`crates/pi-tui/tests/differential_keybindings.rs` drives
`oracle/tui-core-oracle.ts` against the pinned `keybindings.ts` for seven configs:
defaults, submit rebind, select rebind, conflicting user keys, dedupe, unknown id,
and two-way conflict. It compares `getKeys` for every definition,
`getResolvedBindings`, and `getConflicts` exactly.

## Deviations and gaps

- `KeybindingsConfig` is an ordered pair list instead of a JS object. Rust maps are
  either unordered (`BTreeMap` sorts ids) or need an extra dependency; ordering is
  observable in `getConflicts`, so the port keeps the order explicitly.
- `KeyId | KeyId[]` is typed as `Vec<KeyId>`; the typed core excludes the JS union.
- `getResolvedBindings` returns a key list for both one-key and multi-key actions
  instead of `string | string[]`.
- `getKeybindings` returns an Arc-backed manager handle; all holders share mutable bindings. Replacing the registry preserves old-holder identity, matching frozen JS object references. `global_registry_holders_share_mutation_and_replacement_identity` proves both boundaries in its own isolated process.

## Order audit

`migrateKeybindingsConfig` uses a `serde_json::Map` (`preserve_order`) and `js_json::object_entries`,
so JavaScript array-index key ordering and extras' default `Array.prototype.sort` (UTF-16 code units)
are preserved. No reordering remains.

## Q1 re-pin audit (2026-10-21)

Reference: immutable `4f9e6f8a4`. Frozen defaults contain 32 actions; later historyPrevious/historyNext/altScreen actions and ctrl+home/end/pageUp/pageDown aliases removed. Seven-config differential RED then GREEN. Module proof pending final focused gate.

Q1 parent-directed closure: immutable definitions and mutable resolved state are Arc-owned. `KeybindingsManager::clone` preserves identity rather than copying data; no consumer API migration or component edit required. RED `/tmp/q1-global-red.log`, GREEN `/tmp/q1-global-green.log`.
