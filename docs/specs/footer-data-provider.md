# Footer data provider

Frozen `core/footer-data-provider.ts` at the [common pin](../porting/source-lock.json); [branch map](../porting/traceability/footer-data-provider.md). Whole module approved2026-10-24, including its45-line readonly-session-manager prerequisite.

- Resolve regular/worktree/reftable metadata, branch cache and executable names with exact Node text/path semantics.
- Watch/poll/debounce/retry/dispose exactly as frozen; preserve in-flight/pending ordering and cache/notification timing.
- Branch callbacks iterate the live ordered set; removals/additions/disposal during notification take effect immediately. Throwable callbacks retain original errors.
- Statuses remain ordered text/pure callbacks evaluated per read against the current displayed override; omitted output does not remove the carrier.
- Session overrides expose the readonly live session view with isolated object reads and only the allowed goal mutators.
- Available provider count is a JavaScript number, including fractional/negative/nonfinite/negative-zero values, not an integer count restriction.

Current proof is incremental: readonly missing-API/override compile RED→GREEN; numeric domain compile RED→GREEN and live callback actual-source ordering RED→GREEN. Original branch-callback error API compile RED; Result propagation is now implemented with caller adapters. Watcher/refresh-finally/final module acceptance remains open. No new semantic deviation, fallback or dependency is authorized.
