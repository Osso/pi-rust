# Coding-agent auth commands

Auth command behavior ports `packages/coding-agent/src/cli/auth-command.ts` from the pinned TypeScript source. Current coverage includes command metadata, help selection, and pure command parsing.

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

### Command parsing

- [x] Return no command when the first argument is not exactly `auth`.
- [x] Parse exactly `check`, `print-api-key`, and `print-bearer-token`, with source-compatible kinds and defaults.
- [x] Reject missing or unknown subcommands with the exact dynamic source message.
- [x] Preserve every unrecognized argument exactly and in order, including Unicode, NUL, unknown flags, help flags, and duplicates.
- [x] Recognize `--json`, `--credentials`, and `--no-refresh` only for checks, removing them from passthrough arguments and returning exact kind errors otherwise.
- [x] Recognize `--min-expiry` only for bearer-token printing and return exact kind or duration errors in source order.
- [x] Accept the source `/iu` duration unit spellings and preserve the source bug that every accepted non-lowercase spelling uses the hour multiplier.
- [x] Preserve optional expiry presence, repeated-option last-wins behavior, JavaScript `number` precision, and positive infinity with a field-specific `f64`.
- [x] Expose source-compatible auth errors with exact name and message behavior.

## How it works

- [Auth command traceability](../porting/traceability/auth-command.md)

## Implementation inventory

- `crates/pi-coding-agent/src/cli/auth_command.rs` — typed command metadata, help selection, parsing, duration conversion, and errors.
- `crates/pi-coding-agent/src/cli/mod.rs` — public CLI exports.
- `crates/pi-coding-agent/tests/auth_command.rs` — focused metadata, help, parser, error, and expiry-number behavior.
- `oracle/auth-command-oracle.ts` — persistent adapter invoking the real pinned module.
- `crates/pi-coding-agent/tests/differential_auth_command.rs` — direct TypeScript comparison entry point.
- `crates/pi-coding-agent/tests/differential_auth_command/fixtures.rs` — exact metadata and help fixtures.
- `crates/pi-coding-agent/tests/support/oracle.rs` — shared source-lock verification and persistent oracle process client.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/auth_command.rs`
- `crates/pi-coding-agent/tests/differential_auth_command.rs`
- `crates/pi-coding-agent/tests/differential_auth_command/fixtures.rs`

## Known gaps (current cycle)

- [ ] Add direct parser differential proof against the pinned TypeScript module.

## Out of scope

- Help text printing and stdout behavior.
- Full CLI argument validation and the larger `Args` model.
- Credential extraction, provider authentication, and main CLI dispatch.
