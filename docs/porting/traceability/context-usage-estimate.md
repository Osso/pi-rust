# Context usage estimate traceability

Pinned source: `packages/agent/src/harness/compaction/compaction.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `compaction.ts:200-207` | Public mutable interface with required `tokens: number`, `usageTokens: number`, `trailingTokens: number`, and `lastUsageIndex: number \| null` fields | Module-local `harness::compaction::ContextUsageEstimate` with public `f64`, `f64`, `f64`, and `Option<f64>` fields in source order; focused proof plus eight source-shaped differential groups `13148..=13155` |
| `index.ts:58-75` | Package index exports `estimateContextTokens` but omits `type ContextUsageEstimate` | No `harness`-root or package stand-in; Rust remains module-only; direct-module typeproof is separate |

## Rust boundary

- `ContextUsageEstimate` derives `Debug` only and remains publicly mutable.
- TypeScript `number` fields map to unrestricted Rust `f64` values; no production validation, normalization, or integer narrowing is added.
- TypeScript `null` maps to typed Rust `None`; `Some(f64)` preserves negative and fractional indexes and all tested binary64 bit patterns.
- Invalid JavaScript runtime shapes and lone-surrogate strings remain outside the typed Rust boundary.

## Differential proof

- `crates/pi-agent-core/tests/differential_truncate/context_usage_estimate.rs` sends exactly eight contiguous requests `13148..=13155` after request `13147`, preserving existing runner order.
- Fixtures compare final source-shaped plain records, not `estimateContextTokens` or any producer behavior.
- Every numeric position uses hexadecimal binary64 input/output normalization. Null index state remains JSON `null`, distinct from a present `f64` value.
- Coverage includes ordinary values, null and present indexes, negative/fractional indexes, signed zeros, positive/negative subnormals, positive/negative maximum finite values (`7fefffffffffffff`/`ffefffffffffffff`), positive/negative infinities, and distinct positive/negative NaN payloads in all four numeric positions.
- `oracle/truncate-oracle.ts` reconstructs only a local source-shaped plain record and encodes its final values; it does not import or call `estimateContextTokens`, implement producer behavior, or claim runtime construction/validation.

## Direct-module type proof

- `oracle/context-usage-estimate-typecheck.mjs` imports `ContextUsageEstimate` directly from the pinned `compaction.ts` module.
- The standalone fixture proves exact required fields, exact `number | null` index type, public mutation, and unrestricted number assignments.
- No public-index type proof or package/root stand-in is added because `index.ts:58-75` omits the interface.

## Proof status

- Focused RED: `/tmp/pi-rust-context-usage-estimate-red.out` records exit 101 with only the unresolved production import.
- Corrected focused GREEN: `/tmp/pi-rust-context-usage-estimate-green-corrected.out` records exit 0 with 2 passed and 0 failed.
- Differential RED: `/tmp/pi-rust-context-usage-estimate-differential-red.out` records exit 101 at unchanged oracle operation `context_usage_estimate` request `13148`; all prior differential binaries pass.
- Differential GREEN: `/tmp/pi-rust-context-usage-estimate-differential-green.out` records exit 0 with the persistent differential test passed.
- Oracle/typeproof: `/tmp/pi-rust-context-usage-estimate-oracle.out` records `./run-tests.sh oracle` exit 0, including the direct-module proof.
- Node syntax checks for `oracle/truncate-oracle.ts` and `oracle/context-usage-estimate-typecheck.mjs` exit 0.
- Changed Rust/TypeScript/JavaScript readability audits report no threshold violations; no broad workspace, final static, clean-pin, or inherited-verifier claim is made in this phase.

## Exclusions

`estimateContextTokens`, `calculateContextTokens`, message and usage unions, token estimation, compaction orchestration, constructors, production serialization/validation/normalization, integer narrowing, package or harness-root exports, `DEFAULT_COMPACTION_SETTINGS`, dependencies, and unrelated compaction APIs remain deferred.
