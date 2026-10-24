# coding-agent desktop-notification traceability

Pinned source: `packages/coding-agent/src/core/desktop-notification.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: `crates/pi-coding-agent/tests/desktop_notification.rs` runs the notifier in a child process whose `PATH` holds fake `notify-send`, `busctl`, and `setsid` executables, then asserts the exact argv each one received.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `DEFAULT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS` / `NEVER_EXPIRE_DESKTOP_NOTIFICATION_MS` / `PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS` | `-1`, `0`, `999_999` | Same constants; `exposes_the_source_constants` |
| `DesktopNotification` | `body`, `title`, optional `expireTimeMs`, optional `urgency` | `DesktopNotification`; urgency is a typed enum |
| `DesktopNotificationHandle` | `close()` | `trait DesktopNotificationHandle` |
| `DesktopNotifier` | `(notification) => handle \| undefined` | `trait DesktopNotifier` |
| `toDesktopNotificationHandle` | Shape-checks a value for `close` | Not ported: dynamic shape checks have no Rust analogue (typed handle) |
| `sendDesktopNotification` non-Linux | `undefined` | `None` when not Linux |
| notify-send argv | `--print-id`, `--app-name=Pi`, `--expire-time=<ms>`, `--urgency=<u>`, title, body | Same argv; `uses_the_spec_default_expiration_and_critical_urgency`, `preserves_explicit_non_expiring_notifications_and_urgency` |
| Defaults | Expire `-1`, urgency `critical` | `unwrap_or` defaults; same tests |
| `createPendingDesktopNotification` | `close()` before the id resolves defers the close | `NotificationState::request_close`; `defers_close_until_the_notification_id_resolves` |
| `parseNotificationId` | Leading whitespace then digits; unsafe integers rejected | `parse_notification_id` (u32); fake `notify-send` prints ` 42` |
| `closeDesktopNotification` | `busctl --user call … CloseNotification u <id>`, detached and unref'd | `close_desktop_notification` through `setsid`; argv asserted in `preserves_explicit_non_expiring_notifications_and_urgency` |

## Common-pin fallible notification prerequisite (2026-10-24)

At `4f9e6f8a4`, `DesktopNotifier::notify` and `DesktopNotificationHandle::close` return `Result<_, CallbackError>` so callers can retain native Error branding/properties or arbitrary JSON throws. The system notifier remains `Option` wrapped in `Ok`; no native process-spawn behavior changes here. Type-surface RED: `desktop-notify-api-red-2.log` (four expected infallible-interface errors; the first probe also had an unrelated fixture conversion error). `tests/fallible_notifications.rs` exercises error retention and source/native permission-prompt send/close catches. Final proof logs live in `/tmp/pi-rust-repin-core-tools/ledger.md`.

Frozen catch boundaries: ask-questions.ts296-307; mcp-permission-prompt.ts131-153; interactive-mode.ts3824-3831; agents-core/src/runtime.ts2840-2851/2887-2901. All log and continue; native Error diagnostics omit V8 stack frames per the approved gap, retaining error name/message/properties. This prerequisite does not claim whole-module common-pin process parity or change the old proofs above.

Proof at `4341289e`: fallible-notifications3tests, retained permission-prompt23tests and desktop-process5tests pass; the ignored child probe is invoked by its outer tests. Workspace all-target keep-going check has0errors/0warnings. Warning Clippy exits0 with1676 package baseline messages and0 changed-primary-span findings against parent380346b4. Strict protected fixture TS compiler and Biome pass; fmt and manual changed-line Rust/TS readability pass. Inventory PASS686rows/186679lines,451done/19gap/48partial/168todo. Logs: `/tmp/pi-rust-repin-core-tools/desktop-notify-{green,permissions,process,types,biome,inventory}.log` and `{workspace,clippy}-summary.json`. `ExtensionUiContext::notify` and event-bus widening remain held until crate split; no whole ask-questions completion claim.

## Exclusions

- `toDesktopNotificationHandle` is not ported; Rust handles are typed trait objects.

## Discrepancies

- `busctl` is launched through `setsid(1)` to reproduce `detached: true` without `unsafe` (same approved mechanism as `detached-job-bootstrap`); the child is not waited on, matching `unref()`.

## Q18 native notification text (2026-10-22)

Title/body stay JsString through the notifier API, encoding only at notify-send
argv. Test process transport carries lossless JSON strings, not prematurely
encoded notification text; raw title/body fixture checks the actual subprocess
argv sees Node UTF-8 replacement. Fixture written but unexecuted while coding
compilation is blocked. No runtime PASS.
