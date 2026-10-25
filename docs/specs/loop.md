# Recurring session prompts

The bundled loop extension follows the [common source pin](../porting/source-lock.json); [traceability](../porting/traceability/loop.md) maps the full source module.

## What it must do

- [x] Preserve frozen command/tool metadata, interval parsing/formatting, validation and output.
- [x] Send visible custom loop messages with generation/session details, followUp delivery and triggerTurn; allow only one in-flight prompt.
- [x] Defer busy/pending ticks and release deferred work at agent-end only when the frozen pending/in-flight checks permit it. Ignore session continuations and stale generation/session messages.
- [x] Stop replaced/shutdown timers, preserve status/stop behavior, and reject starts after shutdown.
- [x] Preserve exact UTF-16 prompts and serialized details.

## How it works

- [Module traceability](../porting/traceability/loop.md)

## Implementation inventory

- `crates/pi-coding-agent-extensions/src/bundled_extensions/loop/mod.rs` — controller, timer, command and tool.

## Tests asserting this spec

`crates/pi-coding-agent/tests/{loop_runtime,bundled_commands,differential_bundled_extensions}.rs`; `oracle/{loop-runtime,bundled-extensions-a}-oracle.ts`.

## Known gaps (current cycle)

No current behavioral gap in the mapped module.

## Out of scope

JS/libuv scheduling races and numeric timer overflow follow the shared parity exclusions; no live providers are used.
