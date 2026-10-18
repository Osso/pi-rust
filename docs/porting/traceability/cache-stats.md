# coding-agent cache-stats traceability

Pinned source: `packages/coding-agent/src/core/cache-stats.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust module: `crates/pi-coding-agent/src/core/cache_stats.rs`.
Focused tests: `crates/pi-coding-agent/tests/cache_stats.rs` (9, one per source test case).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `CACHE_TTL_MS` | `5 * 60 * 1000` | `CACHE_TTL_MS` |
| `NOISE_FLOOR_TOKENS` | `1024` | `NOISE_FLOOR_TOKENS` |
| `CacheMiss` | missedTokens/missedCost/idleMs/modelChanged | `CacheMiss` |
| `CacheWasteTotals` | missedTokens/missedCost/missCount | `CacheWasteTotals` |
| `ModelPriceSource` | `getModel(provider, modelId) -> {cost:{cacheRead}}` | `ModelPriceSource` + `ModelPrice`; `Models` test source |
| `computeCacheWaste` | cumulative counted misses | `compute_cache_waste`; `accumulates_missed_tokens_and_cost_across_turns`, `counts_nothing_for_healthy_sessions`, `skips_the_turn_after_a_compaction_reset`, `counts_misses_caused_by_model_switches`, `skips_providers_that_report_no_cache_activity` |
| `collectCacheMisses` | counted misses keyed by the paying assistant message | `collect_cache_misses` returns `(entry_index, CacheMiss)`; `maps_counted_misses_to_their_entry_indexes` |
| `detectCacheMiss` | miss on a just-completed message relative to prior entries | `detect_cache_miss`; `detects_a_miss_on_a_just_completed_message_with_idle_time`, `flags_model_switches_on_detected_misses`, `returns_none_for_healthy_turns_and_the_first_turn` |
| `detectMiss` | first turn/reset/no cache activity/noise floor skipped; paid-vs-read rate; idle/model switch | private `detect_miss`; exercised by all cases |
| `asPreviousRequest` | prompt tokens, model key, timestamp, sticky reported-cache flag | private `as_previous_request`; model-switch and zero-cache cases |
| `scan` | compaction/branch-summary resets previous request | private `scan`; reset case |

## Gaps

- `collectCacheMisses` keys by entry index because Rust has no reference-keyed map; callers resolve the assistant message through `entries[index]`.

## Discrepancies

None.
