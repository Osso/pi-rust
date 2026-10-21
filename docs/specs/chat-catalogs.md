# Frozen chat catalog literals

Source: `packages/ai/src/models.generated.ts` and its 36 imported `providers/*.models.ts` at [the common pin](../porting/source-lock.json).

- Embed exactly the exported `MODELS` provider records, including default/GC/Team Codex literals. Preserve provider/model enumeration order, every model value, absent optional fields, and nested arrays/maps.
- Enumerate provider keys and model values directly; lookup uses the literal model key. Missing provider/model returns empty/None. No factories, network generators, old-only catalogs, supplements, or alternate data sources participate in lookup.
- Native `Model` must roundtrip every source model without loss. Catalog costs have four rates; Usage long-write fields do not add catalog rates.
- Operational Codex aliases use the default catalog and rewrite `provider`; they must not replace distinct public literal GC/Team catalogs. This slice does not establish whole factory or mutable-alias parity.
- Artifacts include exact source file SHA-256 and native payload SHA-256 provenance; no invented generation timestamp. No source-absent data-URL API remains.

Staged dependency: timestamp API remains solely because `pi-coding-agent/src/core/model_runtime.rs` imports/calls it. Provenance contains no `generatedAt`, so it returns None. Old generator/checker APIs retain their old grouped-data contracts and are not ported or accepted by this slice. No image catalog changes.

Proof: [traceability](../porting/traceability/providers-models.md). Targeted development proof only; workspace gates belong to integration.
