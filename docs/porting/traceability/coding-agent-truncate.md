# Coding-agent truncate traceability

Frozen source: `packages/coding-agent/src/core/tools/truncate.ts` at `4f9e6f8a4`. Contract: [lossless content](../../specs/lossless-content.md).

Rust: `pi_coding_agent::core::tools::truncate`. Content/result text uses JsString. Constants, options, truncation-cause enum and size formatting retain their existing shared implementations; coding head/tail/line behavior no longer re-exports scalar-only harness functions.

| Capability | Implementation | Current-pin proof / remaining gap |
|---|---|---|
| Head/tail no-truncation and whole-line selection | Retain original UTF-16; Node byte counts determine limits | New `lossless_session` cases await coding-agent compilation; existing scalar differential not rerun yet |
| Partial final-line suffix | Frozen Buffer encoding/UTF-8-boundary decode | Explicit `encode_node_utf8`; raw-suffix case added, not yet executed |
| `truncateLine` | Exact UTF-16 prefix, including a split pair, then marker | New split-pair case added, not yet executed |
| Counts and metadata | Same source line counting and limit precedence | Existing full-metadata differential remains pending on the new API |
| Constants and `formatSize` | Existing shared exports | See [constants](coding-agent-truncation-constants.md) and [format size](coding-agent-format-size.md); no new-pin whole-module claim |

`truncation-red.jsonl` records the focused target's compilation blocker. The former scalar-counting line deviation is no longer used by coding-agent. Harness truncation remains a separate historical surface; this document does not establish its current-pin parity.

Rust limits remain nonnegative integers; negative/fractional/nonfinite JavaScript limits retain their explicit native-type exclusion. No fallback converts internal retained content to scalar strings.
