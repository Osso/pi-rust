# Coding-agent truncate traceability

Frozen source: `packages/coding-agent/src/core/tools/truncate.ts` at `4f9e6f8a4`. Contract: [coding-agent truncation](../../specs/coding-agent-truncate.md), [lossless content](../../specs/lossless-content.md).

Rust: `pi_coding_agent::core::tools::truncate`. Content/result text uses JsString. Constants, options, truncation-cause enum and size formatting retain their existing shared implementations; coding head/tail/line behavior no longer re-exports scalar-only harness functions.

| Capability | Implementation | Current-pin proof / remaining gap |
|---|---|---|
| Head/tail no-truncation and whole-line selection | Retain original UTF-16; Node byte counts determine limits | Current1741-case direct frozen head/tail/line matrix GREEN, including explicit fractional/nonfinite limits |
| Partial final-line suffix | Frozen Buffer encoding/UTF-8-boundary decode | Direct source/native RED for1.25: 't' versus 'st'. Raw double subtraction and fractional Buffer lookup precede shared slice normalization; Unicode/fractional/NaN cases GREEN |
| `truncateLine` | Exact UTF-16 prefix, including a split pair, then marker | Number domain widened to Option<f64>; source fractional contract and native API compile RED, then direct numeric/corpus GREEN. Split-pair/native Buffer boundary fixture GREEN (`accumulator-truncate-units.log`) |
| Counts and metadata | Same source line counting and limit precedence | Full metadata compared in1741 current-pin cases; accumulator additionally asserts exact metadata JSON bytes |
| Constants and `formatSize` | Existing shared exports | See [constants](coding-agent-truncation-constants.md) and [format size](coding-agent-format-size.md); no new-pin whole-module claim |

Historical `truncation-red.jsonl` records the earlier focused target's compilation blocker, superseded by the current GREEN proofs above. The former scalar-counting line deviation is no longer used by coding-agent. Harness truncation remains a separate historical surface; this document does not establish its current-pin parity.

Valid numeric limits are no longer narrowed: head/tail options and single-line maxChars retain binary64. `accumulator-shared-truncate-source-contract.log`, `accumulator-shared-truncate-direct-red.log`, `accumulator-truncate-line-api-red.log` and `accumulator-numeric-shared-green.log` record source/API/behavior proof. Other helpers reviewed: head/tail comparisons already retain f64; the only additional numeric narrowing was truncateLine, corrected in the same slice. Invalid runtime types remain excluded.

DISCREPANCY: fractional byte offsets miss Buffer integer properties, then Buffer.slice floors the offset. This can retain more bytes than requested or decode continuation bytes as U+FFFD; preserve frozen behavior despite its UTF-8-safe comment. No scalar projection of retained text; only the actual Buffer boundary repairs malformed units.
