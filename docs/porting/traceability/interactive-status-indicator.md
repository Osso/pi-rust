# components/status-indicator.ts

Source: `coding-agent/src/modes/interactive/components/status-indicator.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/status_indicator.rs`

Tests: `interactive_components_c.rs::idle_status_*`, `working_status_*`, `retry_status_*`,
`compaction_status_*`, differential `statusIndicator` cases.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `StatusIndicator` base | `StatusIndicator` (`kind`, `set_message`, `dispose`) | differential |
| `WorkingStatusIndicator` | `WorkingStatusIndicator` | static frames test + differential |
| `RetryStatusIndicator` message and countdown | `RetryStatusIndicator` (`poll_countdown`, `next_countdown_deadline`) | formatting test + differential |
| `CompactionStatusIndicator` manual/threshold/overflow labels | `CompactionStatusReason` | differential all three |
| `BranchSummaryStatusIndicator` | `BranchSummaryStatusIndicator` | differential |
| `IdleStatus` two blank lines | `IdleStatus` | height test + differential |
| `{ ...indicator, frames: [] }` static rendering | `LoaderIndicatorOptions` with empty frames | working static test |

## Gaps

- TypeScript `setInterval` is driven by the ported `Loader`/`CountdownTimer` poll API;
  tests advance state explicitly instead of using fake timers.

## Discrepancies

None found.
