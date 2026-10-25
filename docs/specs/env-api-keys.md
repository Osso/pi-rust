# Provider environment credentials

Port frozen `packages/ai/src/env-api-keys.ts` at `4f9e6f8a4`. [Traceability](../porting/traceability/env-api-keys.md) maps its branches.

## What it must do

- [x] Report only configured API-key variable names from the frozen provider map; prefer Anthropic OAuth over its API key.
- [x] Resolve nonempty scoped values before ambient values; empty scoped values fall through, not mask the ambient value.
- [x] For Vertex, require credentials, project and location. Probe nonempty scoped ADC paths directly without updating the cache; otherwise cache the ambient/default-path probe. An explicit API key takes precedence.
- [x] Report Bedrock ambient authentication for the six frozen credential sources, requiring both IAM key fields when that source is used.

## How it works

- [Module traceability](../porting/traceability/env-api-keys.md).

## Implementation inventory

- `crates/pi-ai/src/env_api_keys.rs` — provider mapping and ADC probes/cache.
- `crates/pi-ai/src/utils/provider_env.rs` — shared value resolution.

## Tests asserting this spec

- `crates/pi-ai/tests/env_api_keys.rs`.
- `crates/pi-ai/tests/differential_env_api_keys.rs` — real-file/process and persistent-cache source comparisons.

## Known gaps (current cycle)

None within the approved scope.

## Out of scope

Browser-only ADC caching and Node import-readiness timing are an approved native-runtime gap (2026-10-25). The common parity bar also excludes inherited-prototype lookups and OS/runtime fault shapes.
