# Coding-agent crate layers

The coding-agent runtime is split into Cargo packages without changing its behavior or existing public module paths. Source remains under `crates/pi-coding-agent*/src/`. [Layer graph and boundary changes](../porting/crate-split.md).

## What it must do

- [x] Keep `pi-coding-agent` as the facade and owner of all existing executable, integration-test, and example targets; existing public imports compile unchanged.
- [x] Keep embedded package metadata and byte-identical `pi --version` and `pi --help` output.
- [x] Compile layered packages independently with the existing workspace lints, profiles, and incremental compilation; touch tests rebuild only the changed layer and its dependents.
- [x] Add only internal path packages; preserve external dependency versions and features.
- [x] Keep frozen TypeScript inventory counts unchanged.
- [x] Round 2: touching each layer gives a median-of-three `cargo check -j4 -p pi-coding-agent` in at most 30 seconds, with start/end one-minute load at most 30. Main accepted this metric on 2026-10-24. Record all samples, uptime and debug-build timings; retain the 35.34s tool-support outlier. This is not a hard every-run ceiling.

## How it works

- [Round-2 layer graph and proof](../porting/crate-split-2.md).
- [Round-3 UI layer and build-performance proof](../porting/crate-split-3.md).
- [Round-2 content audit](../porting/crate-split-2-audit.md).

## Implementation inventory

- `crates/pi-coding-agent-base/src/lib.rs`: configuration, utility, console, and runner-mode leaves.
- `crates/pi-coding-agent-data/src/lib.rs`: CLI arguments, permission policy/presets and shared leaves.
- `crates/pi-coding-agent-settings/src/lib.rs`: settings persistence and HTTP/compaction configuration.
- `crates/pi-coding-agent-models/src/lib.rs`: auth, models and model-resolution leaves.
- `crates/pi-coding-agent-storage/src/lib.rs`: SQLite control database, sessions/transcripts and lifecycle storage.
- `crates/pi-coding-agent-packages/src/lib.rs`: package manager and source metadata.
- `crates/pi-coding-agent-tool-support/src/lib.rs`: shared tool algorithms, extension failure values and theme/UI leaves.
- `crates/pi-coding-agent-tools/src/lib.rs`: tools, native extension contracts, shared CLI inventory formatters, resources, shared theme/UI and Supervisor adapters.
- `crates/pi-coding-agent-core/src/lib.rs`: session runtime and compatibility exports for every lower module.
- `crates/pi-coding-agent-extensions/src/lib.rs`: bundled native extensions.
- `crates/pi-coding-agent-ui/src/lib.rs`: widgets, selector model search and login options, independent of bundled extensions.
- `crates/pi-coding-agent-interactive/src/lib.rs`: interactive mode, re-exporting the UI modules.
- `crates/pi-coding-agent/src/lib.rs`: compatibility facade.

## Tests asserting this spec

Existing facade integration tests retain their imports. Checkpoint commands and timing evidence are recorded in [the split proof](../porting/crate-split.md).

## Known gaps (current cycle)

- Parent `5315a68a` fixes the former round-1 session fixtures/isolation. Round 2 requires 72/72 GREEN serial and parallel; the 68/3 exception is retired. See the [proof ledger](../porting/crate-split-2.md).

## Out of scope

Behavior changes, lint changes, and external dependency additions. The separately authorized subagent description macro correction preserves the frozen description bytes and has its own differential test.

## Build-performance round

- [x] Enable incremental compilation for workspace edit loops.
- [x] Keep existing widget/model-search/login-option paths through re-exports while compiling them independently of bundled extensions.
- [Build-performance proof](../porting/build-performance.md).
