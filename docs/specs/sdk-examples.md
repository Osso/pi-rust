# SDK examples

SDK demonstrations follow the frozen `4f9e6f8a4` programs in `packages/coding-agent/examples/sdk/`. [Traceability](../porting/traceability/sdk-examples.md) separates program observer seams from actual SDK state proofs.

## What it must do

- [x] SDK03 replaces the prompt, clears append files, then appends instructions in a second session. Preserve the frozen date/cwd suffix, prompt/output order and disposal.
- [x] SDK09 creates four sessions: default storage/registry, custom storage, shared ephemeral key override and a built-in-only registry sharing default storage. Preserve all labels and disposal order without persisting the override.
- [ ] SDK12 uses custom auth storage but an in-memory registry that ignores models.json, explicit Sonnet/off settings and read/bash tools.
- [ ] Console output matches Node formatting, including string arrays and diagnostic objects; JSON formatting is not a substitute.

- [x] SDK13 queue-output callback failures report on stderr with the frozen session-listener prefix and Error name/message, then continue. No deferred post-transition rejection; V8 stack text is omitted under the approved gap.
- [ ] SDK13 replacement queue ordering awaits removal of native runtime's extra pre-disposal abort; frozen teardown does not emit that queue update.

## How it works

- [Source mappings and proof](../porting/traceability/sdk-examples.md).

## Implementation inventory

- `crates/pi-coding-agent/examples/sdk/`: opt-in native programs.
- `oracle/sdk-examples-oracle.ts`: complete frozen programs, controlled SDK/provider I/O.
- `oracle/sdk-runtime-state-oracle.ts`: actual registry/storage on disposable files.
- `oracle/sdk-resources-state-oracle.ts`: actual resource-loader/session state.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/example_sdk_custom_prompt.rs`
- `crates/pi-coding-agent/tests/example_sdk_api_keys_and_oauth.rs`
- `crates/pi-coding-agent/tests/example_sdk_full_control.rs`
- `crates/pi-coding-agent/tests/example_sdk_session_runtime.rs`: real source queue-listener catch/replacement plus exact stderr child proof.

## Known gaps (current cycle)

- [ ] Current-pin SDK state/oracle migration, remaining resource examples and console formatting.
- [ ] Final lane acceptance and inherited historical gaps remain separate.

## Out of scope

Live provider requests and remote SSH hosts are excluded from verification.
