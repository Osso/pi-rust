# retry-delay.ts

Frozen source: `4f9e6f8a4`, `packages/coding-agent/src/core/retry-delay.ts`.
Rust: `crates/pi-coding-agent/src/core/retry_delay.rs`.

| Export / branch | Proof |
| --- | --- |
| calculateRetryDelayMs: zero, exponential growth, pre/post cap, symmetric jitter, Math.round | `tests/retry_delay.rs::matches_frozen_retry_delay_for_caps_jitter_rounding_and_number_edges`: actual frozen-source differential, including number edges |
| Session consumes increasing attempt and configured cap | `tests/agent_session.rs::repin_settings_retry_delay_grows_with_attempt`: real faux-provider retry events |
| Invalid compaction settings before request | `tests/agent_session.rs::repin_settings_invalid_compaction_threshold_rejects_before_request`: provider response remains unused |

Development proof and exact revisions: `/tmp/pi-rust-repin-settings-callers/ledger.md`. No whole agent-session parity claim. Percentage/idle scheduling, old-pin-only API removal and native automatic settings-save drain remain pending.
