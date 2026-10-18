# Coding-agent truncate traceability

Pinned source: `packages/coding-agent/src/core/tools/truncate.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::truncate` re-exports `pi_agent_core::harness::utils` truncation (see [agent-core truncation](truncation.md)). The coding-agent source differs from `packages/agent/src/harness/utils/truncate.ts` only in UTF-8 byte counting (`Buffer.byteLength`/`Buffer.slice` versus a manual counter with lone-surrogate replacement). Both agree on every valid Unicode string, which is the whole Rust `&str` domain, so no second implementation exists.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `truncate.ts:11-13` | `DEFAULT_MAX_LINES`, `DEFAULT_MAX_BYTES`, `GREP_MAX_LINE_LENGTH` | Existing `truncation_constants.rs`; [coding-agent-truncation-constants](coding-agent-truncation-constants.md) |
| `truncate.ts:15-45` | `TruncationResult`, `TruncationOptions` records | Re-exported structs; `differential_coding_agent_truncate.rs` compares every field |
| `truncate.ts:47-56` | LF-only line counting; terminal LF adds no line | Focused `untruncated_content_preserves_the_trailing_line_feed`; differential corpus with `\n`, `\r\n`, empty content |
| `truncate.ts:61-69` | `formatSize` | Existing [coding-agent-format-size](coding-agent-format-size.md) |
| `truncate.ts:78-158` | `truncateHead`: no-truncation path, oversized first line, complete-line byte/line limits | Focused head tests; differential targeted + corpus cases |
| `truncate.ts:166-249` | `truncateTail`: backward walk, partial last line, line-limit reclassification | Focused tail tests; differential targeted + corpus cases |
| `truncate.ts:255-271` | `truncateStringToBytesFromEnd` cuts on a UTF-8 lead byte | Focused `tail_keeps_a_utf8_safe_suffix_of_an_oversized_last_line`; corpus with 2/3/4-byte scalars and zero byte limits |
| `truncate.ts:277-285` | `truncateLine` default limit and `... [truncated]` marker | Focused marker test; 203 differential line cases (BMP text) |
| `core/tools/index.ts:119-128`, `src/index.ts:333-354` | Tools-index and package-root exports | `pi_coding_agent::core::tools::*` and crate-root re-exports; focused export test |

Differential: `oracle/coding-agent-truncate-oracle.ts` calls the real pinned module; request `27001` sends 1,713 cases (13 targeted, 1,500 seeded head/tail, 200 seeded line cases) and compares full JSON results.

## Deviations (inherited, approved August 12, 2026)

- Limits are `usize`; negative, fractional, NaN, and infinite JavaScript limits are excluded.
- `truncate_line` counts Unicode scalar values; the source counts UTF-16 units and can split a supplementary character into a lone surrogate. The coding-agent function text is identical to the agent harness function that carries this approved deviation, so the differential corpus uses BMP text for line cases.
