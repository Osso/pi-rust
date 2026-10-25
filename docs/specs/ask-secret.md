# Secret prompts

Match frozen `coding-agent/extensions/ask-secret/src/index.ts` at the [common pin](../porting/source-lock.json).

## What it must do

- [ ] Accept exactly one of path/label or record/fields; validate strings and browser field types with frozen messages.
- [ ] Require an interactive TUI; mask file secrets and password fields, preserve field order, and return no secret values.
- [ ] Provision browser credentials with `--fields-json` and a JSON values array on stdin.
- [ ] Save file secrets with a trailing newline, owner-only directories/files, sync-before-rename and temporary-file cleanup. Reject symlink/non-regular destinations.
- [ ] Match cancellation, unavailable results and errors; keep existing destination contents when persistence fails before rename.

## How it works

- [Module trace](../porting/traceability/ask-secret.md).

## Implementation inventory

- `crates/pi-coding-agent-extensions/src/bundled_extensions/ask_secret.rs`: request validation, prompt, provision and file persistence.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/bundled_extensions_tools.rs`: current-pin tool/UI differential and actual file byte/permission differential.

## Known gaps (current cycle)

- [ ] Complete focused proof after the current-pin implementation.

## Out of scope

Wrong-typed/exotic JSON and OS-fault shapes follow the project parity exclusions; no live Secrets Broker test is permitted.
