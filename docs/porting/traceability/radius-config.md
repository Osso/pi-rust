# Old-only Radius provider/config removal

Frozen4f9e6f8a4 has no `ai/src/providers/radius.ts`, `ai/src/providers/radius-config.ts` or `coding-agent/src/core/radius.ts`.

## Outside-consumer audit

Searched every crate, settings/login/model-catalog paths, and docs before deletion. The provider factory and coding-agent synthesis were already removed in the [earlier provider slice](providers-radius.md). Remaining Radius config code was referenced only by its native tests and one lossless SDK test. The coding-agent constant was used only by core_constants and the core-leaves differential/oracle branch. No live settings, /login, catalog or registry consumer remained.

Deleted `providers/radius_config.rs` and export, coding-agent `core/radius.rs` and export, and the old Radius known-provider literal. The six old-only Radius-config mixed differential scenarios were removed in the preceding [OAuth/load slice](load.md). Arbitrary custom ProviderId strings remain supported: resolver tests asserting no Radius default/display entry remain valid. Frozen `orchestrator/src/radius.ts` and its production coordination consumers remain untouched.

## Retired tests/assertions

| File | Exact removal |
|---|---|
| pi-ai/tests/radius_config.rs | normalizes_gateway_urls_with_scheme_detection_and_trailing_slashes; sanitizes_valid_configs_and_filters_shallowly_invalid_models; rejects_non_object_or_malformed_configs; reads_the_gateway_config_from_the_credential_extra_fields; builds_pi_messages_models_from_gateway_entries; loads_gateway_config_with_bearer_auth_and_truncated_errors. Entire old-only file removed. |
| pi-ai/tests/lossless_sdk_boundaries.rs | stored_radius_config_keeps_endpoint_and_rejects_invalid_scalar_model_id only; real HTTP URL/header tests retained. |
| pi-coding-agent/tests/core_constants.rs | Only RADIUS_PROVIDER_ID assertion/import removed; remaining source tool-capability assertions retained, test renamed exposes_tool_capability_constants. |
| pi-coding-agent/tests/differential_core_leaves.rs; oracle/coding-agent-core-leaves-oracle.ts | Only source-absent core/radius.ts dispatch/assertion removed; actual provider-display/tool-capability/session-error differentials retained. |

## KnownProvider contract

`types.rs::KNOWN_PROVIDERS` is the ordered literal union exported by frozen types.ts, not the domain of all custom provider strings. Actual TypeScript compiler API reads the frozen exported union: native41 vs source36 RED, then exact ordered36 GREEN. Removed azure-openai-responses/radius/baseten/qwen-token-plan/qwen-token-plan-cn/qwen-token-plan-individual; inserted openai-codex-team at its frozen position. [Contract](../../specs/known-providers.md).

For each removed id, checked native provider modules, catalog data and registry entries: none remain. Azure's **API adapter** is frozen and stays; custom ProviderId strings using that API remain valid. Old generator code still implements Baseten transforms, three Qwen routes and Azure provider synthesis in model_catalog_generator.rs and model_catalog_generator/{models_dev,catalog,metadata}.rs. Parent explicitly queued a separate whole-module frozen generate-models.ts re-port; this checkpoint does not claim generator parity or delete its unrelated logic.

## Proof

KnownProvider actual compiler-API differential1 + retained HTTP2 GREEN (`q15c-radius-known-provider-green.log`); core constants4 GREEN (`q15c-radius-core-constants-green.log`). Retained actual core-leaves constant differential GREEN1 (`q15c-radius-retained-constants.log`). Code64dbb742 after parentead3bdbc merge: workspace/all-target JSON check0 errors/0 warnings (`q15c-radius-check.{jsonl,stderr}`), warning-mode all-target clippy1228 artifacts0 errors/0 changed-line findings (`q15c-radius-clippy-filtered.json`), inventoryPASS686/186679 and unchanged403/19/87/177 (`q15c-radius-inventory.log`). New compiler oracle tsgo/Biome GREEN (`q15c-radius-oracle-{typecheck,lint}-final.log`); the pre-existing shared core-leaves oracle reports six unchanged-line lint warnings, not suppressed or claimed clean. Manual changed-line Rust/TS readability. No live providers or state used.
