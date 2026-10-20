# Custom Anthropic extension example

Opt-in Rust-native counterpart of pinned `coding-agent/examples/extensions/custom-provider-anthropic/index.ts`. [Traceability](../porting/traceability/example-custom-anthropic.md) records implementation and proof.

## What it must do

- [x] Register two exact model definitions under `custom-anthropic`, custom API `custom-anthropic-api`, OAuth name and API-key environment binding without autoloading.
- [x] Generate 32-byte PKCE, notify the exact authorization URL, prompt once, split `code#state`, exchange/refresh tokens, return access token and expiry minus five minutes; forward callback/fetch/status failures.
- [x] Match API-key/OAuth headers, identity/system prompts, tool casing, messages/images/tool-result grouping, cache control and reasoning budgets.
- [ ] Match complete SDK event sequence, usage/cost replacement, partial JSON behavior, abort/missing-stop/error handling, including temporary content `index` and `partialJson` fields.

## How it works

- [Per-module proof and branch map](../porting/traceability/example-custom-anthropic.md)

## Implementation inventory

- `crates/pi-coding-agent/examples/extensions/custom_provider_anthropic.rs`: embeddable registration and native Provider/OAuth implementation.
- `crates/pi-coding-agent/examples/extensions/custom_provider_anthropic/{request,stream}.rs`: example-specific conversion, headers and SDK event handling.
- `crates/pi-ai/src/api/{anthropic_sdk,anthropic_messages}.rs`: existing SDK request/HTTP and SSE decoder primitives, exposed without behavior changes.
- `oracle/example-custom-anthropic-oracle.ts`: unchanged pinned source bodies evaluated with controlled SDK, fetch, callbacks, random and clock; no public SDK or bundled hooks imported.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/example_custom_anthropic.rs`: actual-file native factory/registry registration, controlled OAuth differential and valid-event boundary regression.

## Known gaps (current cycle)

- [ ] Content-stream matrix blocked only at typed scratch fields; request and terminal/usage/abort/error differentials pass. Full SDK-generated HTTP header proof remains parent-owned.
- [ ] Native content types drop valid temporary source event fields; concrete failing fixture retained, parent coordinating bounded core repair.
- [ ] Parent integration and independent verification pending; coverage remains partial.

## Out of scope

Default SDK/CLI/bundled extension registries, real credentials, live network/login/config/DB, dependency installation, final verification gates. Invalid JavaScript input types excluded only at existing typed boundaries; valid model events are not excluded.
