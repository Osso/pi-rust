# Anthropic provider — common pin

Frozen `packages/ai/src/providers/anthropic.ts`, `4f9e6f8a4`, lines 1–21. Rust `providers/anthropic.rs`.

| Frozen export / branch | Proof |
|---|---|
| `anthropicProvider`, lines 7–20: id/name/baseUrl/catalog/API | `provider_auth::anthropic_provider_matches_frozen_module`: actual frozen descriptor/catalog oracle |
| API-key auth through `envApiKeyAuth`, lines 12–14: OAuth before API key, ignores ANTHROPIC_AUTH_TOKEN | `anthropic_ignores_auth_token_and_resolves_oauth_key`, `anthropic_prefers_oauth_token_over_api_key`; actual frozen auth oracle |
| Stored credential helper semantics | `anthropic_api_key_is_last_and_stored_credential_wins`; [helper traceability](auth-helpers.md) |
| OAuth lazy wrapper name/loader, line 15, no subscription flag | descriptor oracle; frozen helper default false |
| Secret login prompt via env helper | `anthropic_login_prompts_for_a_secret` |

Deleted old-only custom bearer-token implementation, subscription flag and public env constants (constants' old tests deleted as feature deletion). RED 2 before production changes. GREEN 6 provider tests, including actual frozen descriptor and 3 auth scenarios. Revision `0517744e` + provider slice; ledger `/tmp/pi-rust-repin-delta-ai/ledger.md`.

Module factory parity established for current deterministic inputs; shared helper cancellation/in-flight loading gaps remain [explicit](auth-helpers.md). Mark partial rather than asserting those dependencies have whole-module parity. No source bugs identified.

## Common-pin factory closure (2026-10-25)

Scope: entire `ai/src/providers/anthropic.ts` at 4f9e6f8a4, not the implementation of imported API/OAuth/auth-helper modules. Literal factory fields, catalog/provider rewrites and API dispatch mappings were checked directly against frozen source. Current `differential_providers` passes all six source-import comparisons (factory descriptors/catalog, auth resolution, login callbacks, model-aware auth and request dispatch). The collection differential compares every registered chat/image model value and array order with actual `builtinModels`/`builtinImagesModels`. Google Vertex additionally covers all stored-key branches, empty/missing keys owning ambient auth, explicit key and ADC/project/location cases.

No remaining branch in this factory requires a new carrier or behavior change. Imported-module gaps remain tracked in their own module rows. General owned-return approval applies; no frozen built-in consumer mutates these factory return values through a returned reference. Native API/opaque scalar typed exclusions remain unchanged.
