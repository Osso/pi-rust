# Interactive footer

Authority: whole common-pin `modes/interactive/components/footer.ts` ([lock](../porting/source-lock.json)).

- Sum input/output/cache-read/cache-write/cost from assistant message entries only; tool-result usage is excluded. Cache hit rate comes from the latest assistant's prompt usage, and becomes unknown when that prompt total is not positive. Use JS number truthiness, including NaN suppression, for optional token/cost fields.
- Cache session usage, name and context usage until invalidation, session replacement or override changes. Auto-compaction changes rendering immediately without invalidating data. Dispose is an instance no-op; the provider owns watcher cleanup.
- An override supplies its own manager or no manager. A null override manager means zero usage and no session name, never the main session's values. Its cwd/model/thinking/context are independent; omit the main provider's git branch in overridden views. Clearing an override or replacing the session resets cached data.
- Read cwd shortening from `HOME || USERPROFILE`; use exact POSIX Node path resolution/relative containment and preserve raw UTF-16. Only truthy branch/name/executable values add labels. An empty model ID displays `no-model`; an empty thinking-level string remains empty, not `off`.
- Subscription indication is exactly the displayed model provider's live AuthStorage credential type being `oauth`. A null displayed model has no subscription indicator. Do not special-case provider names or use the main model's credentials.
- Preserve token formatting thresholds and JS rounding/toFixed, context unknown/percentage/color thresholds, auto/experimental indicators, model/provider/effort composition, separate dim styles, all width/truncation branches and formatted extension status lines. Retain raw UTF-16 until terminal IO.
- `formatTokens` is source-private; prove it through full footer rendering. `formatCwdForFooter` is exported. Shared Rust numeric/path helpers may be reused by other native consumers without manufacturing a frozen export.

[Whole-module proof](../porting/traceability/interactive-footer.md).
