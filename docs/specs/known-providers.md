# Known provider literals

Frozen `packages/ai/src/types.ts:32–68` exports KnownProvider as an ordered string-literal union. Native public data is `crates/pi-ai/src/types.rs::KNOWN_PROVIDERS`.

## What it must do

- [x] Retain exactly the36 frozen literals in source order, including openai-codex-team; no Radius/Baseten/Qwen/Azure provider literals.
- [x] Continue accepting arbitrary custom ProviderId strings; removal from KnownProvider is not an API-adapter ban.
- [x] Remove source-absent Radius provider/config and coding-agent constant/export consumers; retain real orchestrator Radius coordination.

## How it works

- [Compiler-API proof and outside-consumer audit](../porting/traceability/radius-config.md)

## Implementation inventory

- `crates/pi-ai/src/types.rs`: known literal list; ProviderId remains String.
- `crates/pi-ai/src/providers/mod.rs`, `crates/pi-coding-agent-core/src/core/mod.rs`: no old Radius config/constant exports.

## Tests asserting this spec

- `crates/pi-ai/tests/known_provider_contract.rs`, `oracle/known-provider-contract-oracle.mts`: complete ordered exported contract via TypeScript compiler API.
- `crates/pi-ai/tests/providers.rs`: actual builtin factories, no removed providers.
- `crates/pi-coding-agent/tests/{core_constants,differential_core_leaves,model_resolver}.rs`: retained constants and arbitrary custom-id behavior.

## Known gaps (current cycle)

Whole model-generator re-port is a separate approved slice; this contract does not promote its catalog synthesis.

## Out of scope

- Removing the frozen Azure API adapter or orchestrator Radius module: both exist at the pin.
- Whole types.ts revalidation beyond KnownProvider: separate source row.
