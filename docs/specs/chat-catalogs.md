# Frozen chat catalog literals

Source: `packages/ai/src/models.generated.ts` and its 36 imported `providers/*.models.ts` at [the common pin](../porting/source-lock.json).

- Embed exactly the exported `MODELS` provider records, including default/GC/Team Codex literals. Preserve provider/model enumeration order, every model value, absent optional fields, and nested arrays/maps.
- Enumerate provider keys and model values directly; lookup uses the literal model key. Missing provider/model returns empty/None. No factories, network generators, old-only catalogs, supplements, or alternate data sources participate in lookup.
- Native `Model` must roundtrip every source model without loss. Catalog costs have four rates; Usage long-write fields do not add catalog rates.
- Operational Codex aliases use the default catalog and rewrite `provider`; they must not replace distinct public literal GC/Team catalogs. This slice does not establish whole factory or mutable-alias parity.
- Artifacts include exact source file SHA-256 and native payload SHA-256 provenance; no invented generation timestamp. No source-absent data-URL API remains.

Q15 removed the source-absent coding-agent model_runtime and its timestamp import. Provenance contains no `generatedAt` and is test-only, not a runtime refresh output. The generator is separately re-ported with an [evaluated-module artifact contract](model-catalog-generator.md); the frozen-absent checker/model_data_tools APIs are [removed](model-data-tools.md). No image catalog changes.

Artifact-deletion coupling: builtin provider factory ID/order inventory must match the 36 entries of frozen `providers/all.ts`, including Team, with old-only entries removed. A separate actual-source oracle checks only this inventory; factory/protocol/auth and mutable helper alias parity remain TODO.

Public factory removal: frozen-absent Azure/Baseten/Qwen/Radius factories and their consumers must not remain public. Preserve Azure's frozen API implementation/type export and shared OpenAI APIs. Provider fixtures follow frozen inventory (including Codex Team), not native output. [Removal traceability](../porting/traceability/providers.md#removed-old-only-factories).

Proof: [traceability](../porting/traceability/providers-models.md). All1058literal records and37source-file provenance entries revalidate through the current lossless reader; workspace and changed-line clippy gates cover integration. Separate factory/all.ts/provider protocol/auth work remains independently tracked.
