# Package command errors

Package command execution follows frozen `packages/coding-agent/src/package-manager-cli.ts` at `4f9e6f8a4`. [Trace](../porting/traceability/package-manager-cli.md).

## What it must do

- [x] Report list failures as `Error: <message>` with exit code 1, preserving UTF-16.
- [x] Use global `npmCommand` for self-update, not a trusted project's override.
- [x] Report missing update executables as `spawn <command> ENOENT` and child termination with Node signal names.
- [x] Include the release lookup's original error message in its wrapper.

## How it works

- [Command trace](../porting/traceability/package-manager-cli.md).

## Implementation inventory

- `crates/pi-coding-agent/src/package_manager_cli.rs`: command execution and diagnostics.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/package_command_execution.rs`.

## Known gaps (current cycle)

- [ ] Whole-module closure remains pending.

## Out of scope

Windows-only update behavior follows the repository's platform gap.
