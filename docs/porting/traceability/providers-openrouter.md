# providers/openrouter traceability

## Common-pin factory closure (2026-10-25)

Scope: entire `ai/src/providers/openrouter.ts` at 4f9e6f8a4, not the implementation of imported API/OAuth/auth-helper modules. Literal factory fields, catalog/provider rewrites and API dispatch mappings were checked directly against frozen source. Current `differential_providers` passes all six source-import comparisons (factory descriptors/catalog, auth resolution, login callbacks, model-aware auth and request dispatch). The collection differential compares every registered chat/image model value and array order with actual `builtinModels`/`builtinImagesModels`. Google Vertex additionally covers all stored-key branches, empty/missing keys owning ambient auth, explicit key and ADC/project/location cases.

No remaining branch in this factory requires a new carrier or behavior change. Imported-module gaps remain tracked in their own module rows. General owned-return approval applies; no frozen built-in consumer mutates these factory return values through a returned reference. Native API/opaque scalar typed exclusions remain unchanged.
