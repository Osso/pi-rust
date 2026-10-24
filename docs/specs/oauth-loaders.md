# Built-in OAuth loaders

Frozen `utils/oauth/load.ts` provides three built-in OAuth loaders. Native source is `crates/pi-ai/src/auth/oauth/load.rs`; [mapping/deletion ledger](../porting/traceability/load.md).

## What it must do

- [x] Load Anthropic, OpenAI Codex and GitHub Copilot auth implementations, preserving their current name/method/toAuth contracts.
- [x] Advertise API-key auth only from the OpenRouter image-provider factory; no OAuth handler or unavailable-flow stub.
- [x] Retain frozen Anthropic manual/callback and Codex device/refresh behavior in real-source differential tests.
- [x] Remove old-only OpenRouter/Radius/Kimi/xAI OAuth modules, override registrations/exports and standalone-binary registration hook. No fallback path.

## How it works

- [Static native mapping and outside-consumer audit](../porting/traceability/load.md)
- [OpenRouter image factory](../porting/traceability/openrouter-images-provider.md)

## Implementation inventory

- `crates/pi-ai/src/auth/oauth/load.rs`: three statically linked loaders.
- `crates/pi-ai/src/auth/oauth/mod.rs`, `src/lib.rs`: current module exports.
- `crates/pi-ai/src/providers/openrouter_images.rs`: API-key-only factory.
- `crates/pi-coding-agent/src/bun/cli.rs`: current standalone entry without OAuth registration.

## Tests asserting this spec

- `crates/pi-ai/tests/oauth_load_contract.rs`, `oracle/oauth-load-contract-oracle.mts`: actual loader/factory records.
- `crates/pi-ai/tests/openrouter_images_provider.rs`: catalog/auth/factory behavior.
- `crates/pi-ai/tests/differential_oauth_flows/`, `oracle/oauth-flows-oracle.ts`: retained frozen flow transcripts.

## Known gaps (current cycle)

No loader/factory gap after recorded native representation decisions; final checkpoint gates remain separate.

## Out of scope

- Runtime-imported JavaScript flow/extension identity: Rust-native/static-link representation and approved owned-return semantics.
- Whole old-only Radius provider/config cleanup: next approved slice, after outside-consumer audit. Removed fake differential scenarios are not parity evidence.
