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

Frozen intentionally ignores `thinkingLevel`; requests contain only maxTokens/signal. No core retry API, usage-returning summary helper, generated request session ID, cache override, or retained-tail result remains. Obsolete assertions for those removed APIs were replaced with frozen text/first-kept-ID and exact request assertions; retry-only callback tests were removed. Coding-agent retains its separate retry implementation and no-op callback type.

Native-tree `firstKeptEntryId` replaces embedded retained tails. [Session proof](agent-core-tree-session.md).

Partial dependency: `pi-ai` provider timestamps still use i64 at this checkpoint. Summary/custom creators parse timestamp strings with native Date.parse, including NaN; conversion to typed provider messages explicitly rejects non-integral/nonfinite/signed-zero/out-of-i64 values until the coordinated f64 migration. No clamp or replacement. Remove this temporary boundary with that migration before claiming full parity.

GREEN: `tree-compaction-dev2.log` (31 compaction +13 branch tests), `tree-compaction-oracle-dev2.log` (whole frozen matrix), `tree-native-boundaries-dev.log`. Artifacts under `/tmp/pi-rust-repin-delta-agent-core/`; no final suite claim.
