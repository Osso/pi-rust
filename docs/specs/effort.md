# Effort and delegation mode

Match frozen `coding-agent/extensions/effort/src/index.ts` at the [common pin](../porting/source-lock.json).

## What it must do

- [x] Persist only proactive/disabled modes; display active/disabled and match frozen selector choices and warnings.
- [x] Disabled removes the delegation policy and subagent tools; proactive restores remembered tools in ordered, deduplicated form.
- [x] Ultra activates proactive delegation; disabling it retains max reasoning. Child runtimes cannot change delegation mode.
- [x] Restore branch mode on session start/tree and match exact notifications, status, editor clearing and policy output.

## How it works

- [Module trace](../porting/traceability/effort.md).

## Implementation inventory

- `crates/pi-coding-agent-extensions/src/bundled_extensions/effort/mod.rs`: extension state and command/event handlers.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_bundled_extensions.rs`: frozen handler/tool-state traces.
- `crates/pi-coding-agent/tests/bundled_commands.rs`: native command and branch restoration behavior.

## Known gaps (current cycle)

Focused current-pin handler/tool-state differential and native effort behavior pass; see traceability.

## Out of scope

Wrong-typed/exotic callback values and scheduler races follow the project parity exclusions.
