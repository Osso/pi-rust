# countdown-timer.ts

Frozen `4f9e6f8a4`, `packages/coding-agent/src/modes/interactive/components/countdown-timer.ts:1–38`.
Rust: `crates/pi-coding-agent-ui/src/modes/interactive/components/countdown_timer.rs`.

| Frozen branches | Rust | Proof |
|---|---|---|
| constructor numeric Math.ceil and immediate tick, 14–19 | f64 timeout/remaining_seconds/on_tick; `(timeout_ms / 1000.0).ceil()` | `differential_countdown_numbers` initial signed-zero/fractional/NaN/Infinity/large values |
| 1000ms interval decrement/tick/requestRender, 21–25 | driver-owned next_deadline/poll; subtract `1.0` and invoke same callbacks | same differential three deterministic interval ticks, exact tick strings and request counts |
| <=0 disposal before expiry, 27–30 | comparison/retire/on_expire | same differential active-timer count/expiry order, negative/zero first tick, NaN/Infinity remain live |
| idempotent dispose, 34–38 | clear deadline | numeric differential retirement; `components::countdown_timer_ticks_and_expires` |

No conversion to integer/clamping remains in CountdownTimer, RetryStatusIndicator or interactive retry delay handling. Existing integer-only extension option carriers convert to f64 at their existing boundary; formatting uses JS number rules, not Rust's Infinity/exponent spelling. Those option-carrier APIs are not claimed as newly revalidated here.

RED `test-countdown-numbers-red.log`; GREEN `test-countdown-numbers-green-1.log`1/1 (nine cases), `test-countdown-native-q13-green.log`1/1. Artifacts `/tmp/pi-rust-repin-lossless-interactive/`. Timer scheduling remains driven by the established native TUI poll loop, while differential drives frozen interval callbacks without real one-second sleeps.
