# coding-agent keybindings.ts traceability

Source: `packages/coding-agent/src/core/keybindings.ts` (434 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/core/keybindings.rs`.
The TUI registry itself is `packages/tui/src/keybindings.ts` → `crates/pi-tui/src/keybindings.rs`
(see `keybindings.md`); this module only layers `app.*` bindings on top.

## Exported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `AppKeybindings` interface | ids in `app_keybindings()` | `defaults_bash_backgrounding_and_agent_slots` |
| `AppKeybinding` | `&str` id | same |
| `KEYBINDINGS` (`TUI_KEYBINDINGS` spread + `app.*`) | `keybindings()` = `tui_keybindings()` + `app_keybindings()` | differential `load` requests compare the full effective config |
| `KeybindingsManager` | `KeybindingsManager` wrapping `pi_tui::keybindings::KeybindingsManager` | focused + differential |
| `KeybindingsManager.create(agentDir)` | `KeybindingsManager::create(Option<&Path>)` | `loads_legacy_key_names_before_the_file_is_rewritten`, differential `load` |
| `reload` | `reload` | `reload_rereads_the_keybindings_file` |
| `getEffectiveConfig` | `get_effective_config` | differential `effective` |
| `migrateKeybindingsConfig` | `migrate_keybindings_config` | differential `migrate` (legacy rename, collision keeps namespaced value, extras, array-index ordering) |
| `Keybinding`, `KeyId`, `KeybindingsConfig` re-exports | `pi_tui::keybindings` types | compile-time reuse |
| private `orderKeybindingsConfig` | `order_keybindings_config` + `with_javascript_key_order` | differential `migrate` |
| private `loadRawConfig` / `toKeybindingsConfig` | `load_raw_config` / `to_keybindings_config` | differential `load` (invalid JSON, non-object, invalid values) |

## Branches

- Platform defaults: `app.suspend` is empty on Windows; `app.clipboard.pasteImage` is `alt+v` on Windows;
  tree fold/unfold key order is macOS-specific. Rust uses `cfg!(target_os = ...)`; Linux CI exercises the
  non-Windows/non-macOS branch.
- Unknown ids, non-string/non-string-array values, missing file, and invalid JSON are dropped, matching
  `toKeybindingsConfig` and the `loadRawConfig` catch.
- JavaScript array-index keys are enumerated first in numeric order (`with_javascript_key_order`).

## Gaps

- Windows and macOS default-key branches are not executed on this host; they are literal ports of the
  `process.platform` conditionals.
- `getKeys`/`getConflicts` delegate to the already-ported TUI manager; this module adds no new conflict logic.

## Proof

- Focused: `cargo test -p pi-coding-agent --test keybindings` (3 passed).
- Differential: `cargo test -p pi-coding-agent --test differential_keybindings` (1 passed) via
  `oracle/coding-agent-keybindings-oracle.ts`, comparing user bindings, effective bindings, and resolved keys
  for migration cases and file loads.
