# Coding-agent crate layers

The coding-agent runtime is split into Cargo packages without changing its behavior or existing public module paths. Source remains under `crates/pi-coding-agent*/src/`. [Layer graph and boundary changes](../porting/crate-split.md).

## What it must do

- [ ] Keep `pi-coding-agent` as the facade and owner of all existing executable, integration-test, and example targets.
- [ ] Preserve values, process state, error behavior, public paths, embedded package metadata, and byte-identical `pi --version` and `pi --help` output.
- [ ] Compile layered packages independently with the existing workspace lints, profiles, and disabled incremental compilation.
- [ ] Add only internal path packages; preserve external dependency versions and features.
- [ ] Keep frozen TypeScript inventory counts unchanged.

## How it works

- [Layer graph, moved support leaves, and proof](../porting/crate-split.md).

## Implementation inventory

- `crates/pi-coding-agent-base/src/lib.rs`: configuration, utility, console, and runner-mode leaves.
- `crates/pi-coding-agent-core/src/lib.rs`: core runtime and shared CLI, Supervisor, and UI leaves.
- `crates/pi-coding-agent-extensions/src/lib.rs`: bundled native extensions.
- `crates/pi-coding-agent-interactive/src/lib.rs`: interactive mode.
- `crates/pi-coding-agent/src/lib.rs`: compatibility facade.

## Tests asserting this spec

Existing facade integration tests retain their imports. Checkpoint commands and timing evidence are recorded in [the split proof](../porting/crate-split.md).

## Known gaps (current cycle)

- [ ] Complete checkpoint verification after the structural changes and parent integration.

## Out of scope

Behavior changes, incremental compilation, profile/lint changes, external dependency additions, and further core splitting before round 1 lands.
