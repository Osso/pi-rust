# Custom Anthropic extension example

Opt-in Rust-native counterpart of pinned `coding-agent/examples/extensions/custom-provider-anthropic/index.ts`. [Traceability](../porting/traceability/example-custom-anthropic.md) records implementation and proof.

## What it must do

- [x] Register two exact model definitions under `custom-anthropic`, custom API `custom-anthropic-api`, OAuth/provider display name and API-key environment binding without autoloading; preserve source registration adapter's configured-key checks, source labels and missing-env errors.
- [x] Generate 32-byte PKCE, notify the exact authorization URL, prompt once, split `code#state`, exchange/refresh tokens, return access token and expiry minus five minutes; forward callback/fetch/status failures.
- [x] Match API-key/OAuth headers, identity/system prompts, tool casing, messages/images/tool-result grouping, cache control and reasoning budgets.
- [x] Preserve literal producer `index` and `partialJson` lifecycle: first matching source index, removal only at block stop, catch removes indices but retains partial JSON, successful unclosed blocks retain scratch; malformed JSON retains last valid object arguments.
- [x] Reject parsed null, arrays and scalar tool arguments with `Tool arguments must be a JSON object`, emitting an error after start/toolcall_start, preserving the raw buffer and last object arguments. Approved deviation: source accepts arbitrary JSON; native typed arguments are object-only.
- [x] Prove installed-SDK actual HTTP auth/compatibility headers, body and events; native runtime identity and exactly four native transport defaults are approved differences ([decision and wire proof](../porting/traceability/example-custom-anthropic.md#approved-native-transport-defaults)).

## How it works

- [Per-module proof and branch map](../porting/traceability/example-custom-anthropic.md)

## Implementation inventory

- `crates/pi-coding-agent/examples/extensions/custom_provider_anthropic.rs`: embeddable registration and native Provider/OAuth implementation.
- `crates/pi-coding-agent/examples/extensions/custom_provider_anthropic/{request,stream}.rs`: example-specific conversion, headers and SDK event handling.
- `crates/pi-ai/src/api/{anthropic_sdk,anthropic_messages}.rs`: existing SDK request/HTTP and SSE decoder primitives, exposed without behavior changes.
- `oracle/example-custom-anthropic-oracle.ts`: unchanged pinned source bodies evaluated with controlled SDK/fetch/callbacks/random/clock; explicit wire mode loads only the installed Anthropic SDK against isolated loopback fixtures. No public Pi SDK or bundled hooks imported.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/example_custom_anthropic.rs`: actual-file native factory/registry registration, controlled OAuth differential and valid-event boundary regression.

## Known gaps (current cycle)

- [ ] Parent integration and independent verification pending; coverage remains partial.

## Out of scope

Default SDK/CLI/bundled extension registries, real credentials, live network/login/config/DB, dependency installation, final verification gates. Invalid JavaScript input types excluded only at existing typed boundaries; valid model events are not excluded.
