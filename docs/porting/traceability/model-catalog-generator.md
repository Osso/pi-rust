# Model catalog generator traceability

Source: `packages/ai/scripts/generate-models.ts` at original runtime pin. No exports; private functions and automatic CLI invocation form the contract. Rust: `crates/pi-ai/src/model_catalog_generator.rs`. Native JSON artifact adaptation approved 2026-10-20.

| Source workflow/symbol | Rust/proof | Status |
|---|---|---|
| `getModelsDevCost` | `get_models_dev_cost`; original TS costs/tiers differential | partial: valid fixtures |
| `processBasetenModels` | `process_baseten_models`; empty/deprecated/plain/toggle/effort/both/GLM fixtures | partial: valid fixtures |
| `processFireworksModels` | `process_fireworks_models`; empty/tool filter/GLM/Kimi/Anthropic fixtures | partial: valid fixtures |
| `recordModelsDevReasoningOptions` | pending metadata-state integration | gap |
| `readGeneratorOptions`, automatic CLI | pending | gap |
| NVIDIA/OpenRouter/AI Gateway HTTP | oracle fixture-only HTTP installed; Rust pending | gap |
| `loadModelsDevData` (all remaining providers) | pending | gap |
| compat/thinking/tool-search/cache metadata helpers | pending | gap |
| hardcoded supplements, overrides, Azure clones | pending | gap |
| priority/dedup/sorted provider/API/model catalogs | pending | gap |
| native artifacts/manifest/hash/staging/validation/rollback/stats | pending shared model-data integration | gap |

## Proof

`cargo test -p pi-ai --test model_catalog_generator -- --nocapture`: RED expected null vs source cost object; GREEN 1 test with 11 original-source requests. Logs: `/tmp/pi-rust-model-generator-proof/02-red.log`, `03-green.log`. Oracle transpiles actual pinned source with TypeScript, removes automatic invocation via AST and exposes unchanged private functions. Process group/watchdog uses existing `support/oracle.rs`. No network refresh or source writes.

## Representation and exclusions

Raw provider values use shared `JsValue`/`JsString`, preserving UTF-16 code units and JS object enumeration. Existing typed reasoning converter is reused, not cloned; unknown efforts cannot produce a supported Pi effort. No runtime-model migration. Lone-surrogate and malformed raw-payload proof remain open; do not treat valid-fixture coverage as whole-module parity.
