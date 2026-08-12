# Protocol-schema traceability

Pinned source: `packages/protocol/src/schemas.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Direct differential proof invokes the real pinned TypeBox schemas. Accepted schema-leaf values compare exact literals or strings; rejected values compare schema acceptance. Accepted model costs compare exact IEEE-754 bits.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `schemas.ts:5` | `IdSchema` accepts strings with source `minLength: 1` | Focused and direct TypeBox ordinary, Unicode, whitespace, combining, embedded-NUL, and empty-field fixtures |
| `schemas.ts:7-8` | Strict objects reject additional fields | Typed-constructor exclusion; future JSONL or private wire decoders must prove this boundary |
| `schemas.ts:24-33` | `ThinkingLevelSchema` accepts exactly seven literals and excludes `ultra` | Focused round-trip test and direct TypeBox fixtures cover every literal plus five invalid spellings |
| `schemas.ts:35-45` | `SessionPhaseSchema` accepts exactly five literals | Focused round-trip test and direct TypeBox fixtures cover every literal plus five invalid spellings |
| `schemas.ts:47-50` | `ModelRefSchema` declares `provider` then `id`, both using `IdSchema` | Focused preservation and Rust diagnostic-order tests; direct TypeBox fixtures cover five accepted and three rejected pairs |
| `schemas.ts:53` | `ModelCostSchema` field order is input, output, cacheRead, cacheWrite | Focused first-invalid-field test; differential object fields use source names |
| `schemas.ts:54` | `input` accepts finite `number >= 0` | Focused integer-valued/finite/invalid tests; TypeBox finite-number, fractional, negative, and NaN fixtures |
| `schemas.ts:55` | `output` accepts finite `number >= 0` | Focused fractional/finite/invalid tests; TypeBox fractional, negative, and infinity fixtures |
| `schemas.ts:56` | `cacheRead` accepts finite `number >= 0` | Focused zero/subnormal/invalid tests; TypeBox subnormal, negative, and infinity fixtures |
| `schemas.ts:57` | `cacheWrite` accepts finite `number >= 0` | Focused negative-zero/invalid tests; TypeBox exact-bit and NaN fixtures |
| TypeBox number/minimum guards | Reject NaN, infinities, and negative values; accept negative zero | 6 focused tests plus 11 direct TypeBox differential fixtures |
| TypeBox string/minLength guards | Every Rust-representable non-empty string satisfies `minLength: 1`; empty strings fail | Focused and direct TypeBox `ModelRef` fixtures; JS-only lone surrogates documented as unrepresentable |
| `schemas.ts:60-64` | `ModelMetadataSchema` starts with non-empty provider, id, name, and api strings | Focused preservation and source-order empty-field tests; direct TypeBox proof pending |
| `schemas.ts:65-66` | `reasoning` is boolean; `input` is an array of exact `text|image` literals with no minimum length | Typed bool/enum fields; focused empty/order/duplicate/literal tests; invalid runtime types excluded; direct TypeBox proof pending |
| `schemas.ts:67-68` | `contextWindow` and `maxTokens` are integer-valued numbers greater than or equal to 1 | Focused `1`, `u64::MAX`, zero, and source-order tests; TypeBox values above `u64::MAX` are an approved typed-core exclusion; direct shared-domain proof pending |
| `schemas.ts:69-71` | Cost uses `ModelCostSchema`; supported thinking levels are non-empty; authenticated is boolean | Existing typed dependencies plus focused preservation and empty-level tests; direct TypeBox proof pending |
