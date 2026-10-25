# coding-agent keybindings.ts traceability

Source: `packages/coding-agent/src/core/keybindings.ts` (434 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent-data/src/core/keybindings.rs`.
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
  `oracle/coding-agent-keybindings-oracle.ts`, comparing migrated config entries, the ordered resolved
  binding ids, and each id's resolved keys in source order. `getUserBindings` keeps the Rust `Vec<(id, Vec<KeyId>)>` shape and is
  covered by the focused tests instead of a shape-normalizing oracle comparison.

## Native app registry and keybinding-file JSON

`app_keybindings`, combined definitions, migration/config ordering, registry IDs/keys/descriptions, stored file path and match/get lookups retain `JsString`; file JSON uses `JsObject`/`JsValue` and `parse_lossless`. Raw user key overrides are not discarded by serde scalar-string parsing. Source object ordering still uses JS array-index ordering and native UTF16 comparison for extra keys. Path encoding occurs only in `reload` immediately at the real filesystem call.

`raw_app_keybindings` loads actual JSON files with high/low/pair overrides and compares frozen `KeybindingsManager.create` getKeys/getUserBindings. The proof-only standalone harness imports the actual production module; its ambient `get_agent_dir` dependency deliberately panics (tests provide explicit directories). Whole coding-agent library remains blocked by independently owned consumers; native module compilation RED is recorded separately from behavioral acceptance.

## Q18 native UTF-16 integration (2026-10-22)

KeybindingsManager::create accepts Option<&JsString>; stored config_path remains native through reload. Only actual filesystem lookup/read encodes it. Frozen keybindings.ts407 reads UTF-8 with Node replacement; Rust read_to_string still rejects malformed UTF-8 bytes. That existing decoding difference is a re-pin gap, not changed in this UTF-16 slice. No primary diagnostics at interactive6; runtime tests remain unexecuted.

## Baseline registry re-pin (2026-10-25)

Frozen default app keybindings omit app.message.copy; remove that extra native default. The loaded-config differential was RED on precisely that extra action ID. Explicit user mappings remain governed by the existing manager.
