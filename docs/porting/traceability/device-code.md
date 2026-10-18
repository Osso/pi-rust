# auth/oauth/device-code traceability

Pinned source: `packages/ai/src/auth/oauth/device-code.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| Constants | `Login cancelled`, `Device flow timed out`, slow-down clock-drift message, 1000 ms minimum, 5 s default, 5 s slow-down increment. | Private constants in `device_code`; `error_messages_match_the_source`. |
| `abortableSleep` | Sleep rejects with `Login cancelled` when the signal aborts before or during the wait. | `abortable_sleep` over `CancellationToken`; `cancels_an_in_flight_wait`. |
| `pollOAuthDeviceCodeFlow` | Deadline from `expiresInSeconds`; interval `max(1000, floor(interval*1000))`; optional pre-poll wait; poll loop; `complete` returns, `failed` throws its message, `slow_down` counts and uses a finite positive server interval or adds 5 s; sleeps `min(interval, remaining)`; timeout message depends on whether any `slow_down` occurred. | `poll_oauth_device_code_flow`; `polls_immediately_and_returns_the_completed_value`, `can_wait_before_the_first_poll`, `increases_the_interval_by_five_seconds_after_slow_down`, `honors_a_server_provided_slow_down_interval`, `times_out_without_slow_down_responses`, `reports_the_clock_drift_message_after_slow_down_timeout`, `surfaces_a_failed_poll_message`, `enforces_a_one_second_minimum_interval`. |

## Exclusions and deviations

- The poll callback returns the result enum directly; a rejecting promise maps to `Failed { message }` at the adapter boundary. The TypeScript function propagates arbitrary rejection values unchanged.
- Deadlines use `tokio::time::Instant`, so Tokio paused-time tests reproduce the fake-timer behavior. Non-finite intervals/expiry are excluded by the typed API; negative or non-finite durations clamp to zero.
- No differential oracle: behavior depends on timers and abort scheduling; deterministic Tokio tests cover the branches.
- No source bug found.
