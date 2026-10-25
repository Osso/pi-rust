# compaction-threshold.ts

Frozen `4f9e6f8a4`, `packages/coding-agent/src/core/compaction-threshold.ts` (13 lines).
Rust: `crates/pi-coding-agent-settings/src/core/compaction_threshold.rs`.

| Export / branch | Rust contract | Behavioral proof |
| --- | --- | --- |
| `parseCompactionThresholdPercent(undefined)` | `parse_compaction_threshold_percent(None)` returns `Ok(None)` | focused helper test; manager differential defaults |
| finite number in `(0,100]` | `Some(&Value::Number)` returns percentage | focused 0.001/50/100; differential 0.01/100 |
| zero/negative/>100 | `Err(SettingsFailure)` | focused exact error text; differential getter ERROR |
| present null/string/bool/array/object | invalid, JS `String(value)` in message | focused concrete values; differential null/string |
| non-finite numbers | not representable by JSON-backed `Settings`; no coercion to a valid percentage | explicit Rust JSON boundary; not a direct non-finite runtime oracle claim |

2026-10-25 whole common-pin helper audit and direct 14-input frozen differential GREEN at 2683e5cbc: `cargo test -j3 -p pi-coding-agent --test suite_core_2 compaction_threshold_helper` (`/tmp/ext-api-suite_core_2-compaction_threshold_helper.log`). No current realistic-input delta.

Manager delegates validation; missing threshold omits `thresholdPercent` from public oracle observations. Retry/percentage scheduling is not implemented by this helper. See [settings contract](../../specs/settings-manager.md) and [manager traceability](settings-manager.md).

Proof status: targeted GREEN at 2acf3e82 + per-step harness flush correction: direct frozen helper differential (14 inputs), native helper/error tests, manager validation snapshots. RED manager differential rejected old acceptance of zero percentage in `/tmp/pi-rust-repin-settings/red-validation.txt`. Non-finite native inputs remain outside JSON representation; no direct NaN/infinity runtime parity claim.

Mutation: changing strict lower bound `>0` to `>=0` is killed by the direct frozen helper oracle at input0; `/tmp/pi-rust-repin-settings/mutation-threshold-zero.txt`.
