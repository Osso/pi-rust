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
