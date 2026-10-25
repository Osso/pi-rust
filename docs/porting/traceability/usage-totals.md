# coding-agent usage-totals traceability

Pinned source: `packages/coding-agent/src/core/usage-totals.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust module: `crates/pi-coding-agent-core/src/core/usage_totals.rs`.
Focused tests: `crates/pi-coding-agent/tests/usage_totals.rs` (3).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `UsageTotals` | input/output/cacheRead/cacheWrite/cost | `UsageTotals` |
| `createUsageTotals` | all zero | `create_usage_totals`; `creates_and_accumulates_usage_totals` |
| `addUsageToTotals` | adds usage fields and `cost.total` | `add_usage_to_totals`; same test |
| `UsageCostBreakdownEntry` | `{key, cost, tokens}` | `UsageCostBreakdownEntry` |
| `getUsageCostBreakdown` | assistant usage keyed `provider/responseModel|model`, tool-result usage in `Tools/summaries`, zero buckets filtered, cost-descending stable sort | `get_usage_cost_breakdown`; `groups_usage_by_model_and_tool_bucket`, `uses_response_model_when_present` |

## Gaps

None. Bucket iteration preserves first-seen order for equal costs, matching `Map` iteration plus stable sort.

## Discrepancies

None.
