# Protocol-schema traceability

Pinned source: `packages/protocol/src/schemas.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: `differential_protocol::model_cost` invokes the real pinned `ModelCostSchema` through TypeBox `Check`. Accepted values compare exact IEEE-754 bits; rejected values compare schema acceptance.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `schemas.ts:7-8` | Strict objects reject additional fields | Typed-constructor exclusion; future JSONL or private wire decoders must prove this boundary |
| `schemas.ts:53` | `ModelCostSchema` field order is input, output, cacheRead, cacheWrite | Focused first-invalid-field test; differential object fields use source names |
| `schemas.ts:54` | `input` accepts finite `number >= 0` | Focused integer/finite/invalid tests; TypeBox integer, fractional, negative, and NaN fixtures |
| `schemas.ts:55` | `output` accepts finite `number >= 0` | Focused fractional/finite/invalid tests; TypeBox fractional, negative, and infinity fixtures |
| `schemas.ts:56` | `cacheRead` accepts finite `number >= 0` | Focused zero/subnormal/invalid tests; TypeBox subnormal, negative, and infinity fixtures |
| `schemas.ts:57` | `cacheWrite` accepts finite `number >= 0` | Focused negative-zero/invalid tests; TypeBox exact-bit and NaN fixtures |
| TypeBox number/minimum guards | Reject NaN, infinities, and negative values; accept negative zero | 6 focused tests plus 11 direct TypeBox differential fixtures |
