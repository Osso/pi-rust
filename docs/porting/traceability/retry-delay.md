# retry-delay.ts

Frozen source: `4f9e6f8a4`, `packages/coding-agent/src/core/retry-delay.ts`.
Rust: `crates/pi-coding-agent-data/src/core/retry_delay.rs`.

| Export / branch | Proof |
| --- | --- |
| calculateRetryDelayMs: zero, exponential growth, pre/post cap, symmetric jitter, Math.round, Math.min signed-zero ties | `tests/retry_delay.rs::matches_frozen_retry_delay_for_caps_jitter_rounding_and_number_edges`: actual frozen-source differential; independent signed-zero counterexamples added after initial 108-case proof |
| Session consumes increasing attempt and configured base | `tests/agent_session.rs::repin_settings_retry_delay_grows_with_attempt`: two real faux-provider retry events; cap saturation at this consumer is not yet tested |
| Invalid compaction settings before next request after assistant response | `tests/agent_session.rs::repin_settings_invalid_compaction_threshold_rejects_before_next_request`: second response remains unused after first nonterminal text response; removing next-turn validation fails the assertion |

## Native fallibility propagation

These are Rust `Result` adaptations of source throws, not new error-swallowing policy. The following boundaries compile in the focused agent-session tests, but lack dedicated invalid-setting fault-injection cases unless mapped above:

- `will_retry_after_agent_end` and `prepare_retry` propagate invalid retry settings to their event/continuation caller; `retry_policy` propagates into local compaction.
- `compaction_settings`, manual/automatic compaction, ready-cache matching/taking, and tool-turn background preparation propagate invalid compaction settings. The detached background installer logs failures, matching the source catch; it does not return them to the original prompt caller.
- `ErrorStatusScheduler::schedule -> Result<(), HandlerError>` propagates session/settings access and retry-validation failures. Missing optional settings uses zero base as source does.
- `GoalIdleSelection.schedule_error_status` returns `Result`; the no-pending-input model-error branch propagates it through `select_goal_for_idle_review` and the registered handler. The pending-input branch reports deferral without invoking the error scheduler. Dedicated synchronous failure injection remains unproven; existing timer-error proof does not cover it.

A fresh initial request is not unconditionally preflighted by prepare-next-turn; existing assistant history can reach pre-prompt checks. Percentage/idle scheduling, old-pin-only API removal and native automatic settings-save drain remain pending. No whole agent-session or goal-extension promotion.

Development and independent proof, exact revisions, and known failures: `/tmp/pi-rust-repin-settings-callers/{ledger.md,VERIFY.md}`.

## Whole-module revalidation (2026-10-25)
All 16 frozen lines match the native zero-base/exponential/cap/jitter/round/final-cap branches. Existing actual-source differential passes at 2683e5cbc (`/tmp/settings-models-retry-delay.log`); production/test unchanged through 02e7c6033. Targeted warning-mode data-library clippy passes. Numeric extremes/runtime oddities are outside the current parity bar; existing edge fixtures are retained, not expanded. Consumer gaps above belong to their respective modules, not this pure helper.
