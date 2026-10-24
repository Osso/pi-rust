# Live compaction display

Frozen `interactive-mode.ts:579,3939–3948,5083–5107` governs the live display.

- After compaction, display the returned summary, tokens before/after, kept-context and compacted-result token counts, and duration; then render context entries except compactions. Do not substitute persisted summary metadata or initial-load notices/history/footer updates.
- Cache the latest context compaction entry ID, complete result, and display timestamp. No compaction entry means no cache, but still display the returned summary.
- Editing that ID updates the summary while retaining the live metadata and timestamp. Other edits rebuild initial messages from persisted entries, leaving the cache unchanged.
- Successful fork, tree navigation and current-session reset clear pending editing and the live display. Cancelled transitions retain them.
- Text stays exact UTF-16. No old persisted-only rebuild path remains for live compaction.

Proof: `interactive_live_compaction.rs` and actual-source `interactive-live-compaction-oracle.mts`; navigation/fork probes are recorded separately in the module traceability.
