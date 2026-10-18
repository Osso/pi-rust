# coding-agent tool-detach-registry traceability

Pinned source: `packages/coding-agent/src/core/tool-detach-registry.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: focused behavioral tests in `crates/pi-coding-agent/tests/tool_detach_registry.rs`, including real timers (20 ms) for auto-detach, cancellation, and one timer per registration. Differential testing is not applicable: the module holds live handles and timers.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `ToolDetachHandle` | `detach(): boolean` | `trait ToolDetachHandle { fn detach(&self) -> bool }` |
| `ToolDetachRegistryOptions.autoDetachAfterMs` | Optional override, default `600_000` | `with_auto_detach_after_ms`, `DEFAULT_AUTO_DETACH_AFTER_MS`; `default_auto_detach_matches_the_source` |
| `register` | Adds the handle, starts an unref'd auto-detach timer, returns an unsubscribe function | `register` returns `DetachSubscription`; `reports_running_handles_and_unregisters_them`, `auto_detach_skips_handles_unregistered_before_the_timeout` |
| `detachRunning` | Iterates newest → oldest and stops at the first `true` | `detach_running` over a reversed snapshot with `any`; `detaches_the_most_recently_registered_running_handle`, `detach_running_reports_when_no_handle_accepts_detachment` |
| `hasRunning` | Non-empty handle set | `has_running` |
| `startAutoDetachTimer` | After the delay, detach only if still registered | Thread + `recv_timeout` cancellation channel; `auto_detaches_still_registered_handles_after_the_timeout`, `every_registration_gets_its_own_timer` |
| `unregister` | Removes the handle and clears its timer | `DetachSubscription::unregister`/`Drop`; cancellation test |

## Exclusions

- `register` takes `self: &Arc<Self>` so the timer thread can hold a `Weak` handle list.
- The timer is a detached `std::thread` blocked on `recv_timeout` instead of `setTimeout(...).unref()`; it never keeps the process alive, matching `unref()`.
- Handles are `Arc<dyn ToolDetachHandle>` because the source stores object references.

## Discrepancies

None.
