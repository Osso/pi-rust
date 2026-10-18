# elapsed-time.ts

Source: `coding-agent/src/modes/interactive/components/elapsed-time.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/elapsed_time.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `MIN_VISIBLE_ELAPSED_MS` | `MIN_VISIBLE_ELAPSED_MS` | differential `interactive-components-oracle.ts` (`elapsed`) |
| `< 1s` → `Nms` | `format_elapsed_duration` first branch | differential values 0, 1, 999, -5 |
| `< 60s` → `Ns` | second branch | differential 1000, 1500, 59999 |
| `< 60m` → `Nm SSs` | third branch | differential 60000, 61000, 3599999 |
| `>= 60m` → `Nh MMm` | fourth branch | differential 3600000, 3661000, 123456.789 |
| `padStart(2, "0")` | `pad_two_digits` (`js_number_to_string` + `utf16_len`) | differential minute/hour cases |
| `Math.max(0, value)` | `duration_ms.max(0.0)` | differential -5 |

No gaps. Non-finite inputs are not representable in the JSON oracle; the Rust code keeps `NaN`/`Infinity` flowing through the same arithmetic as JS.
