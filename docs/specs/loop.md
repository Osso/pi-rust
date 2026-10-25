# Recurring session prompts

The bundled loop extension follows the [common source pin](../porting/source-lock.json); [traceability](../porting/traceability/loop.md) maps the full source module.

## What it must do

- [ ] Preserve frozen command/tool metadata, interval parsing/formatting, validation and output.
- [ ] Send visible custom loop messages with generation/session details, followUp delivery and triggerTurn; allow only one in-flight prompt.
- [ ] Defer busy/pending ticks and release deferred work at agent-end only when the frozen pending/in-flight checks permit it. Ignore session continuations and stale generation/session messages.
- [ ] Stop replaced/shutdown timers, preserve status/stop behavior, and reject starts after shutdown.
- [ ] Preserve exact UTF-16 prompts and serialized details.

## How it works

- [Module traceability](../porting/traceability/loop.md)

## Implementation inventory

- `crates/pi-coding-agent-extensions/src/bundled_extensions/loop/mod.rs` — controller, timer, command and tool.

## Tests asserting this spec

`crates/pi-coding-agent/tests/{loop_runtime,bundled_commands,differential_bundled_extensions}.rs`; `oracle/{loop-runtime,bundled-extensions-a}-oracle.ts`.

## Known gaps (current cycle)

- [ ] Execute focused GREEN after the implementation commit.

## Out of scope

JS/libuv scheduling races and numeric timer overflow follow the shared parity exclusions; no live providers are used.
