# Frozen compaction.ts

Source: [common pin](../source-lock.json), `agent-core/src/harness/compaction/compaction.ts:1–846`.
Rust: `crates/pi-agent-core/src/harness/compaction/mod.rs`.

| Export/branch | Proof |
|---|---|
| Token estimates, context usage, last assistant usage, thresholds and default settings | `compaction.rs`; corresponding `differential_compaction` cases |
| Cut points, turn starts, bounded protected suffix, latest active user floor and metadata rewind | Same differential; focused large-user/tool-cycle tests |
| `prepareCompaction`, latest summary, `firstKeptEntryId`, kept-history boundary, split turn and file details | Native-tree focused tests and frozen preparation differential |
| `generateSummary`, initial/update/custom prompts, raw UTF-16 clipping, token caps, errors and aborts | Frozen faux-provider differential; `lossless_provider_errors` |
| `compact`, history-only/prefix-only/split output, files, first-kept ID, errors and aborts | Same differential and 31 focused compaction tests |
| `CompactionDetails`, `CompactionSettings`, `ContextUsageEstimate`, `CutPointResult`, `SUMMARIZATION_SYSTEM_PROMPT` | Retained public record tests plus runtime differential |

Frozen intentionally ignores `thinkingLevel`; requests contain only maxTokens/signal. No core retry API, usage-returning summary helper, generated request session ID, cache override, or retained-tail result remains. Obsolete assertions for those removed APIs were replaced with frozen text/first-kept-ID and exact request assertions; retry-only callback tests were removed. Coding-agent summary retries and the temporary no-op callback type were also retired in the [rejectable acquisition integration](stream-acquisition.md).

Native-tree `firstKeptEntryId` replaces embedded retained tails. [Session proof](agent-core-tree-session.md).

Provider-message timestamps now retain f64. Summary/custom creators parse timestamp strings with native Date.parse, including NaN; typed conversion no longer rejects non-integral/nonfinite/signed-zero values. [Timestamp integration and bounded proof](message-timestamps.md).

GREEN: `tree-compaction-dev2.log` (31 compaction +13 branch tests), `tree-compaction-oracle-dev2.log` (whole frozen matrix), `tree-native-boundaries-dev.log`. Artifacts under `/tmp/pi-rust-repin-delta-agent-core/`; no final suite claim.

## Public index number domain

[Contract](../../specs/compaction-index-domain.md). Public findTurnStartIndex/findCutPoint now accept f64 and return NativeNodeError when frozen array access throws. Direct numeric loop/dereference order preserves NaN/empty/fractional ranges and signed zero rather than pre-clamping to usize. Protected suffix calculations keep their bounded usize representation; prepareCompaction derives its bounds from the entry slice and calls the same public helper with that invariant.

RED `compaction-index-domain-red.log` exposed the usize-only infallible API. GREEN `compaction-index-domain-green1.log` and `compaction-index-domain-regression.log`:765 direct frozen cases (full/metadata/empty arrays),31 existing focused tests, and the unchanged whole frozen compaction differential. This closes the last index-domain proof gap. The parent-reported old find_cut_point differential RED was not reproduced: that target was already GREEN in `compaction-index-existing-red.log` before this slice, despite the artifact filename; no assertions were weakened to make it pass.

