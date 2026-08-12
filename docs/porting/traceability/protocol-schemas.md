# Protocol-schema traceability

Pinned source: `packages/protocol/src/schemas.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `schemas.ts:7-8` | Strict objects reject additional fields | Typed-constructor exclusion; future decoders must prove this boundary |
| `schemas.ts:53` | `ModelCostSchema` field order is input, output, cacheRead, cacheWrite | `model_cost.rs::reports_the_first_invalid_source_field` |
| `schemas.ts:54` | `input` accepts finite `number >= 0` | Integer/fractional/finite and invalid-input focused cases |
| `schemas.ts:55` | `output` accepts finite `number >= 0` | Integer/fractional/finite and invalid-output focused cases |
| `schemas.ts:56` | `cacheRead` accepts finite `number >= 0` | Zero/subnormal and invalid-cache-read focused cases |
| `schemas.ts:57` | `cacheWrite` accepts finite `number >= 0` | Negative-zero bit preservation and invalid-cache-write focused cases |
| TypeBox number/minimum guards | Reject NaN, infinities, and negative values; accept negative zero | `model_cost.rs` non-finite, negative, and negative-zero cases |

Direct TypeBox differential proof is pending in this cycle.
