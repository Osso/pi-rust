# Native footer arithmetic helpers

Frozen 4f9e6f8a4 has no `coding-agent/src/core/usage-totals.ts`. Retired the old-only UsageCostBreakdownEntry/get_usage_cost_breakdown API and its two grouping/response-model tests. The export had no production callers.

The native module retains only UsageTotals/create_usage_totals/add_usage_to_totals as arithmetic support for the actual interactive footer. The footer passes assistant usage only; its behavior and the unchanged `creates_and_accumulates_usage_totals` assertions are preserved.

Old Rust-written tool-result usage data remains readable and byte-preserved by the existing raw session-record path even though it is not a typed ToolResultMessage field. See [type retirement](ai-types.md) and `lossless_session_persistence::old_rust_tool_result_usage_survives_lossless_disk_read_and_rewrite`.
