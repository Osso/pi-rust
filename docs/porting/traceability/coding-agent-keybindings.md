# Coding-agent keybindings

Source: common pin `4f9e6f8a4`, `coding-agent/src/core/keybindings.ts`, all 444 lines. Native: `crates/pi-coding-agent-data/src/core/keybindings.rs`. [Contract](../../specs/coding-agent-keybindings.md). The underlying registry is independently mapped in [TUI keybindings](keybindings.md).

## Whole-module map (2026-10-25)

| Frozen surface / branch | Native mapping | Behavioral proof |
|---|---|---|
| `AppKeybindings`, `AppKeybinding`, `KEYBINDINGS`, type reexports | App catalog layered after the TUI definitions; existing native key/config types | Complete ordered exported definition/default/description differential; background and agent-slot units |
| Legacy table; `migrateKeybindingsConfig` renamed/unchanged/collision branches | `keybinding_name_migrations`, `migrate_keybindings_config` | Five concrete source migration fixtures; explicit namespaced value wins; values retained |
| `orderKeybindingsConfig` known order, extras, JS numeric enumeration | `order_keybindings_config`, native UTF-16 comparison and JS object enumeration | Exact source entries for known/unknown/alphabetic/numeric ids; existing raw app-keybinding corpus |
| `isRecord`, `toKeybindingsConfig`, `loadRawConfig`, `loadFromFile` | Existing object/string/string-array filter, lossless JSON loader and source catch behavior | Four source load cases: legacy/mixed/malformed/missing file; existing raw file fixtures |
| Constructor, `create(agentDir)`, config path, `getEffectiveConfig` | Manager around the existing shared TUI authority; Node path join and config-dir authority | Full loaded effective-id/key order snapshots; file-loading units |
| `reload` with/without config path | Reload current file through the same TUI manager; no-path return | Eight source/native snapshots: new override, conflicting legacy override, deletion, duplicate/empty overrides; user/effective/conflict/match results and final unchanged file bytes |
| Inherited matching/conflict/user/resolved behavior | Delegated to the mapped TUI authority | Reload snapshots plus underlying registry proofs; scalar/array JS key unions normalize to native key lists |

RED: `/tmp/repin-interactive-2-proof/keybindings-red.log` exposes the extra native `app.message.copy` id. Expanded RED `keybindings-definitions-red.log` compares the actual exported data, including every visible description. The port removes that old-only definition/action, the clipboard description suffix and the obsolete macOS fold/unfold ordering branch. Parent confirmed that old-only feature removal is the re-pin rule; `/copy` is unchanged. GREEN `keybindings-green.log` covers complete definitions, five migrations, four file loads and eight reload snapshots.

File formats are read-only here: migration returns the original values in frozen key order, and loading/reload does not write the input bytes. UTF-16 ids, keys, descriptions and config paths retain existing `JsString`/`JsValue` carriers through the actual filesystem boundary. Owned returned values and typed Rust shape exclusions follow project policy; no alternate path or compatibility table is added.

## Recorded exclusions

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

Gap: Windows suspend/paste defaults are not executed on this Linux host; malformed UTF-8 configuration files and prototype-inherited lookups are pathological/runtime-oddity exclusions under the user-observable policy. No new proof matrix or carrier is introduced for them.

## File decoding (compat re-audit 2026-10-26)

Source reads with `readFileSync(path, "utf-8")` (keybindings.ts:404-408); native reads bytes and decodes with U+FFFD replacement of invalid sequences (same rule as Node), so a stray Latin-1 byte no longer drops the file. RED/GREEN: `differential_keybindings::invalid_utf8_bytes_decode_like_node_and_keep_overrides` (/tmp/config-utf8-red*.log, /tmp/config-utf8-green.log).
