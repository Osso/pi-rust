# providers/github-copilot traceability

Source: packages/ai/src/providers/github-copilot.ts at [common pin](../source-lock.json).
Rust: crates/pi-ai/src/providers/github_copilot.rs.

| Frozen behavior | Native | Proof |
|---|---|---|
| id/name/baseUrl, token env auth, lazy OAuth name, three API entries | github_copilot_provider | differential_providers provider descriptors and dispatch |
| No subscription flag or model filter (:10–26) | default lazy flag, no filter | a12f44de actual-source RED: subscription true and picker list empty; literal port removes both |
| Full catalog independent of credential.availableModelIds | default unfiltered models | provider_auth credential matrix; source descriptor pickerModels comparison |

Historical pre-repin filter/subscription claims are retired, not preserved as compatibility paths. Current focused GREEN/checkpoint results: [Q15 proof](../q15-sdk-provider-proof.md).

## Common-pin factory closure (2026-10-25)

Scope: entire `ai/src/providers/github-copilot.ts` at 4f9e6f8a4, not the implementation of imported API/OAuth/auth-helper modules. Literal factory fields, catalog/provider rewrites and API dispatch mappings were checked directly against frozen source. Current `differential_providers` passes all six source-import comparisons (factory descriptors/catalog, auth resolution, login callbacks, model-aware auth and request dispatch). The collection differential compares every registered chat/image model value and array order with actual `builtinModels`/`builtinImagesModels`. Google Vertex additionally covers all stored-key branches, empty/missing keys owning ambient auth, explicit key and ADC/project/location cases.

No remaining branch in this factory requires a new carrier or behavior change. Imported-module gaps remain tracked in their own module rows. General owned-return approval applies; no frozen built-in consumer mutates these factory return values through a returned reference. Native API/opaque scalar typed exclusions remain unchanged.
