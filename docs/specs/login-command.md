# Terminal OAuth login

Port frozen `4f9e6f8a4` `cli/login-command.ts` through the public AuthStorage/OAuth registry. [Trace](../porting/traceability/login-command.md).

## What it must do

- [ ] Preserve dispatch/help/flag validation, provider ordering, output destinations and exit codes, including empty provider ids.
- [ ] Forward auth/device/progress/prompt/manual/select callbacks with exact text and answers; save credentials with the frozen AuthStorage spread/key order.
- [ ] Preserve buffered input CRLF splitting and a final bare carriage return.
- [ ] Await optional browser opening without changing successful login on browser failure; the default browser-cli argv/error text must match.
- [ ] Use frozen default-Chalk stdout color capability for both success and error text.

## How it works

- [Module/source proof](../porting/traceability/login-command.md).
- [Persistence contract](auth-storage.md).

## Implementation inventory

- `crates/pi-coding-agent/src/cli/login_command.rs`: dispatch, input, callback bridge and browser helper.
- `crates/pi-coding-agent-models/src/core/auth_storage.rs`: public login and credential publication.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/login_command.rs` and `tests/support/login_command_oracle.rs`.
- `oracle/login-command-oracle.ts`: unchanged source command/storage/registry with private fixtures.

## Known gaps (current cycle)

Focused GREEN is pending for the current implementation commit.

## Out of scope

Node readline cursor editing/history and independent runtime scheduling, explicitly approved2026-10-25; OS/runtime fault shapes and pathological inputs under AGENTS.md. Prompts, answers, output and persistence are not excluded.
