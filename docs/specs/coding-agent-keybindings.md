# Coding-agent keybindings

Match the user-visible catalog, migration and file-loading behavior of frozen `coding-agent/src/core/keybindings.ts` at `4f9e6f8a4`. [Module map](../porting/traceability/coding-agent-keybindings.md).

## What it must do

- [x] Expose the complete frozen ordered TUI/app catalog with exact default keys and visible descriptions; do not add an `app.message.copy` action. Tree fold/unfold ordering is platform-independent.
- [x] Rename legacy keys, preserve an explicitly configured namespaced value, retain unknown extra values and order keys like JavaScript objects.
- [x] Load valid string/array overrides; absent, malformed or non-object files produce empty overrides as in the frozen loader.
- [x] Reload current file contents, including deletion and repeated keys, through the same registry. Without a configured file path, reload keeps existing bindings.
- [x] Preserve source conflict ordering and key matching; never rewrite the configuration file while loading or migrating it in memory.

## How it works

- [Coding-agent module map](../porting/traceability/coding-agent-keybindings.md)
- [TUI registry authority](../porting/traceability/keybindings.md)

## Implementation inventory

- `crates/pi-coding-agent-data/src/core/keybindings.rs` — catalog, legacy migration, loader and manager.
- `crates/pi-coding-agent-interactive/src/modes/interactive/interactive_mode.rs` — source app-action bindings; `/copy` remains a separate command.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_keybindings/mod.rs` and `oracle/coding-agent-keybindings-oracle.ts` — complete exported definitions, migrations, loads and repeated reload/conflict/match/file-byte snapshots.
- `crates/pi-coding-agent/tests/keybindings.rs` — actual file loading/reload and default background/agent-slot bindings.
- `crates/pi-coding-agent/tests/raw_app_keybindings.rs` — existing UTF-16 file/config boundary proofs.

## Known gaps (current cycle)

No realistic POSIX behavior gap is identified in this module. Excluded inputs/platforms are recorded once in the module map.

## Out of scope

Windows-only defaults are not executed on this Linux host. Pathological malformed UTF-8 configuration files and prototype-inherited JavaScript lookup behavior follow the project parity exclusions; existing lossless carriers remain unchanged.
