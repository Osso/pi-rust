# Commands extension example

Opt-in Rust-native counterpart of pinned
`packages/coding-agent/examples/extensions/commands.ts`. Native extension
embedding is approved; no automatic loading or default product behavior changes.

## What it must do

- [x] Register only `/commands`, with exact description and ordered source-prefix completions; no matches returns null.
- [x] Query commands at invocation time, JS-trim arbitrary source filters, and emit the exact empty-list information message.
- [x] Group extensions, prompts, skills in that order, retaining order within each group; format truthy descriptions and select under `Available Commands`.
- [x] Ignore cancellation, empty selection, and headers; split selected names at ` - `, remove one UTF-16 code unit, and find the first original command, even outside the filter.
- [x] Confirm truthy source paths with the exact prompt; notify the path only on acceptance.

## How it works

- [Embedding guide](../../crates/pi-coding-agent/examples/extensions/README.md)
- [Module proof and branch map](../porting/traceability/example-commands.md)

## Implementation inventory

- `crates/pi-coding-agent/examples/extensions/commands.rs`: explicitly embedded registration function and native handler.
- `oracle/example-commands-oracle.ts`: actual pinned module with controlled API/UI fixture, ASCII-LF JSON framing.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/example_commands.rs`: actual shipped source compilation, native registry/factory/action binding, exact TS metadata/completions/full action transcripts.

## Known gaps (current cycle)

- [ ] Parent's independent final verification pending; coverage row remains partial.

## Out of scope

- Dynamic TypeScript loading, standalone executable, autoloading, default registry changes, live sessions/config/DB, provider/network calls.
- Invalid JavaScript runtime types and unpaired-surrogate strings cannot inhabit native typed inputs; valid scalar strings remain exact, without oracle normalization.
