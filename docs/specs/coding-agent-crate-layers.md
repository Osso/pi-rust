# Coding-agent crate layers

The coding-agent runtime is split into Cargo packages without changing its behavior or existing public module paths. Source remains under `crates/pi-coding-agent*/src/`. [Layer graph and boundary changes](../porting/crate-split.md).

## What it must do

- [x] Keep `pi-coding-agent` as the facade and owner of all existing executable, integration-test, and example targets; existing public imports compile unchanged.
- [x] Keep embedded package metadata and byte-identical `pi --version` and `pi --help` output.
- [x] Compile layered packages independently with the existing workspace lints, profiles, and disabled incremental compilation; touch tests rebuild only the changed layer and its dependents.
- [x] Add only internal path packages; preserve external dependency versions and features.
- [x] Keep frozen TypeScript inventory counts unchanged.
- [ ] Round 2: touching each layer must give `cargo check -j4 -p pi-coding-agent` in at most 30 seconds at comparable load. Record uptime and debug-build timings; split further if the limit is exceeded.

## How it works

- [Current layer graph and proof](../porting/crate-split-2.md).
- [Round-2 content audit](../porting/crate-split-2-audit.md).

## Implementation inventory

- `crates/pi-coding-agent-base/src/lib.rs`: configuration, utility, console, and runner-mode leaves.
- `crates/pi-coding-agent-data/src/lib.rs`: CLI arguments, permission policy/presets and shared leaves.
- `crates/pi-coding-agent-settings/src/lib.rs`: settings persistence and HTTP/compaction configuration.
- `crates/pi-coding-agent-models/src/lib.rs`: auth, models and model-resolution leaves.
- `crates/pi-coding-agent-storage/src/lib.rs`: SQLite control database, sessions/transcripts and lifecycle storage.
- `crates/pi-coding-agent-packages/src/lib.rs`: package manager and source metadata.
- `crates/pi-coding-agent-tool-support/src/lib.rs`: shared tool algorithms, extension failure values and theme/UI leaves.
- `crates/pi-coding-agent-tools/src/lib.rs`: tools, native extension contracts, resources, shared theme/UI and Supervisor adapters.
- `crates/pi-coding-agent-core/src/lib.rs`: session runtime and compatibility exports for every lower module.
- `crates/pi-coding-agent-extensions/src/lib.rs`: bundled native extensions.
- `crates/pi-coding-agent-interactive/src/lib.rs`: interactive mode.
- `crates/pi-coding-agent/src/lib.rs`: compatibility facade.

## Tests asserting this spec

Existing facade integration tests retain their imports. Checkpoint commands and timing evidence are recorded in [the split proof](../porting/crate-split.md).

## Known gaps (current cycle)

- Parent `5315a68a` fixes the former round-1 session fixtures/isolation. Round 2 requires 72/72 GREEN serial and parallel; the 68/3 exception is retired. See the [proof ledger](../porting/crate-split-2.md).

## Out of scope

Behavior changes, incremental compilation, profile/lint changes, and external dependency additions. The separately authorized subagent description macro correction preserves the frozen description bytes and has its own differential test.
