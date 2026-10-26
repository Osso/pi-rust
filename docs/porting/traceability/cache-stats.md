# Removed old-only cache statistics

The [common pin](../source-lock.json) has no `coding-agent/src/core/cache-stats.ts`. The prior-pin Rust module was not current parity evidence.

Parent explicitly assigned whole removal on2026-10-22: deleted `crates/pi-coding-agent-core/src/core/cache_stats.rs`, its core module export and `tests/cache_stats.rs`. Removed interactive notice call sites/helpers/price adapter and cache-waste/cost-breakdown rendering. The old positive notice test was replaced by actual MessageStart→MessageEnd regression: a preserved unknown legacy flag cannot emit a notice.

The 2026-10-22 removal also deleted cache-notice and Mermaid-mode settings paths. Cache-notice paths remain removed; the separately authorized historical Mermaid setting and selector were later restored ([trace](interactive-mermaid.md)). Do not strip arbitrary legacy settings JSON: unknown-key persistence is a separate source contract.

The `/session` replacement runs the actual frozen private handler and compares complete native Text frames, not a rewritten source formatter. [Contract](../../specs/session-info.md) · [Handler proof](interactive-mode.md#session-info-and-old-only-cache-removal-2026-10-22).

`test-cache-notice-removal-red-1.log` observed the old `Cache miss after 10m idle` output; `test-cache-notice-removal-green-1.log` proves its absence after the same real event boundary. No frozen source bug or new semantic deviation is claimed.
