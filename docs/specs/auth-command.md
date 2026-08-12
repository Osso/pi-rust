# Coding-agent auth command metadata and help

Auth command metadata and help selection port `packages/coding-agent/src/cli/auth-command.ts` from the pinned TypeScript source. This bounded slice covers command kinds, names, usage text, and help detection only.

## What it must do

### Command metadata

- [x] Represent the `check`, `api_key`, and `bearer_token` auth command kinds as typed Rust variants.
- [x] Return the exact pinned command name for every kind.
- [x] Return the exact pinned usage string for every kind.

### Help selection

- [x] Reject help selection unless the first argument is exactly `auth`.
- [x] Select help when the second argument is absent or exactly `help`.
- [x] Select help when any argument is exactly `--help` or `-h`.
- [x] Match case-sensitively without normalizing Unicode, NUL-containing strings, duplicates, or argument order.

## How it works

- [Auth command traceability](../porting/traceability/auth-command.md)

## Implementation inventory

- `crates/pi-coding-agent/src/cli/auth_command.rs` — typed command metadata and pure help selection.
- `crates/pi-coding-agent/src/cli/mod.rs` — public CLI exports.
- `crates/pi-coding-agent/tests/auth_command.rs` — focused metadata and help behavior.
- `oracle/auth-command-oracle.ts` — persistent adapter invoking the real pinned module.
- `crates/pi-coding-agent/tests/differential_auth_command.rs` — direct TypeScript comparison entry point.
- `crates/pi-coding-agent/tests/differential_auth_command/fixtures.rs` — exact metadata and help fixtures.
- `crates/pi-coding-agent/tests/support/oracle.rs` — shared source-lock verification and persistent oracle process client.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/auth_command.rs`
- `crates/pi-coding-agent/tests/differential_auth_command.rs`
- `crates/pi-coding-agent/tests/differential_auth_command/fixtures.rs`

## Known gaps (current cycle)

None for this bounded metadata/help slice.

## Out of scope

- Auth command parsing, duration conversion, and parse errors.
- Help text printing and stdout behavior.
- Full CLI argument validation and the larger `Args` model.
- Credential extraction, provider authentication, and main CLI dispatch.
