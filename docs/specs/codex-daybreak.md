# Codex Daybreak Blue

The bundled Daybreak extension follows the [common source pin](../porting/source-lock.json). Branch coverage is mapped in its [traceability](../porting/traceability/codex-daybreak.md).

## What it must do

- [x] Register `/daybreak` with exact command text; support blue/off/query/usage, main-thread restrictions, selection notifications and editor clearing.
- [x] Restore the last recognized session custom entry and update active/inactive status on session start and model selection.
- [x] For supported Codex providers and eligible request model ids, merge `access_programs.cyber = daybreak_blue`; preserve other request properties and raw UTF-16 values.
- [x] Keep the program off by default and leave unsupported or child requests unchanged; warn on nonobject payloads only after the frozen guards pass.

## How it works

- [Module traceability](../porting/traceability/codex-daybreak.md)

## Implementation inventory

- `crates/pi-coding-agent-extensions/src/bundled_extensions/codex_daybreak.rs` — command, state, persistence and request hook.
- `crates/pi-coding-agent-extensions/src/bundled_extensions/mod.rs` — first-party factory registration.
- `crates/pi-coding-agent/src/main.rs` — first-party name inventory.

## Tests asserting this spec

`crates/pi-coding-agent/tests/codex_daybreak.rs`, `oracle/codex-daybreak-oracle.ts` and the first-party registration assertion in `crates/pi-coding-agent/tests/bundled_extensions_commands.rs`.

## Known gaps (current cycle)

No behavioral gap remains in the mapped module; extension-wide clippy is blocked by existing errors in other modules.

## Out of scope

Live provider access and pathological/runtime fault shapes are excluded by the lane task and repository parity bar.
