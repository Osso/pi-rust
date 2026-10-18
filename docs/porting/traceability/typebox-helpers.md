# utils/typebox-helpers traceability

Pinned source: `packages/ai/src/utils/typebox-helpers.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `StringEnum(values, options?)` | Builds a TypeBox `Type.Unsafe` schema with `type: "string"`, an `enum` array, and optional `description`/`default`. | **Gap**: no Rust item. The function only adapts the TypeBox JSON-schema builder; the Rust JSON-schema representation is still an open design decision, so porting it now would create a second, incompatible schema model. |

## Gap rationale

- The module has no behavior beyond constructing a TypeBox schema object for downstream provider request bodies.
- Port it together with the JSON-schema design that replaces TypeBox for provider tool definitions; the coverage row records status `gap`.
