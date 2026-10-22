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

- TypeScript `setInterval` is driven by the ported `Loader`/`CountdownTimer` poll API; tests advance state explicitly instead of using fake timers.
Retry attempt/maxAttempts/delayMs and countdown seconds now use f64 throughout. No fractional/negative/non-finite exclusion is inferred.

## Q13 whole-module audit

All 112 frozen lines compared. Subclass composition formerly hid inherited setMessage/kind and advertised input handling although frozen Loader has none. Native wrappers now dereference their StatusIndicator and report no input handling; disposal delegates through the approved trait hook. `status_indicator_api` RED missing methods → GREEN1/1: all four subclasses update exact unpaired UTF-16 messages, expose correct kind, avoid input handling, and dispose retry countdown without render requests.

`differential_interactive_components::status_indicators_match_the_pinned_typescript_source` GREEN1/1 compares the eight themed status cases independently of unrelated leaf modules. Artifacts: `test-status-api-red.log`, `test-status-api-green.log`, `test-status-differential-q13-green.log` under `/tmp/pi-rust-repin-lossless-interactive/`. Scoped native timer lifetime is proved in [native disposal](native-component-disposal.md).

Numeric RED `test-countdown-numbers-red.log` demonstrates the missing f64 constructor/callback surface. GREEN `test-countdown-numbers-green-1.log`: numeric differential1/1 over nine cases (negative/fractional delays and counters, negative zero, NaN, ±Infinity, exponent formatting and above-safe-integer values); subclass API1/1. Actual pinned CountdownTimer and RetryStatusIndicator run under a deterministic interval driver, comparing initial/tick values, expiry, render requests, complete frames, live timer count and retirement over three ticks. Math.ceil preserves negative zero; decrement/comparison preserve NaN/Infinity; all labels use shared JS number formatting. [Countdown mapping](countdown-timer.md).

`test-countdown-native-q13-green.log` native tick/expiry1/1 and `test-status-numeric-differential-q13-green.log` themed eight-case differential1/1 on the final f64 surface. Full 112-line module is covered, without changing frozen timer arithmetic.

## Native status text

Working frame options, status messages, style inputs/outputs, render lines and input forwarding retain `JsString`. Retry/compaction/branch labels use shared `js_format!`, formatting only primitive numeric values before native interpolation; no native text encoding. Idle rows are native spaces.

`raw_status_indicator` compares actual frozen StatusIndicator with raw high/low/pair messages, direct custom callbacks, setter updates, six width/update snapshots and request-render side effects. The standalone proof imports the actual production module and actual CountdownTimer; unused ambient theme/retry-hint dependencies deliberately panic. It proves the direct-callback base class, not themed subclass/timer raw behavior; existing broader component tests remain the source for those branches until full coding integration compiles.
