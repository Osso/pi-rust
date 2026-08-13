# Context usage estimate traceability

Pinned source: `packages/agent/src/harness/compaction/compaction.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `compaction.ts:200-207` | Public mutable interface with required `tokens: number`, `usageTokens: number`, `trailingTokens: number`, and `lastUsageIndex: number \| null` fields | Module-local `harness::compaction::ContextUsageEstimate` with public `f64`, `f64`, `f64`, and `Option<f64>` fields in source order; focused field-literal, mutation, optional-state, Debug, and binary64-bit tests |
| `index.ts:58-75` | Package index exports `estimateContextTokens` but omits `type ContextUsageEstimate` | No `harness`-root or package stand-in; Rust remains module-only |

## Rust boundary

- `ContextUsageEstimate` derives `Debug` only and remains publicly mutable.
- TypeScript `number` fields map to unrestricted Rust `f64` values; no validation, normalization, or integer narrowing is added.
- TypeScript `null` maps to typed Rust `None`; `Some(f64)` preserves negative and fractional indices and all tested binary64 bit patterns.
- Invalid JavaScript runtime shapes and lone-surrogate strings remain outside the typed Rust boundary.

## Proof status

- Focused RED: `/tmp/pi-rust-context-usage-estimate-red.out` captures exit 101 with only the unresolved module import for the missing `ContextUsageEstimate` API.
- Focused GREEN: `/tmp/pi-rust-context-usage-estimate-green.out` captures exit 0 with 2 passed and 0 failed after the minimal module record was added.
- Formatting, readability, differential/oracle, broad workspace, final static, and clean-pin gates are not claimed in this production phase.

## Exclusions

`estimateContextTokens`, `calculateContextTokens`, message and usage unions, token estimation, compaction orchestration, constructors, serialization, validation, normalization, integer narrowing, package/root exports, `DEFAULT_COMPACTION_SETTINGS`, and unrelated compaction APIs remain deferred.
