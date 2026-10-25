# provider-composer — removed old-only architecture

`packages/coding-agent/src/core/provider-composer.ts` is absent from frozen `4f9e6f8a4`.
Q15 deletes `crates/pi-coding-agent-core/src/core/provider_composer.rs` and its old-only tests.
No provider composition wrapper/fallback is retained. The frozen
[model-registry](model-registry.md) owns ProviderConfigInput, config overlays and registration.
Consumer migration and verification remain open.
