# server/errors.ts traceability

Source: `packages/server/src/errors.ts`. Rust: `crates/pi-server/src/errors.rs`, re-exported from `lib.rs`.

| TS export | Rust | Proof |
|---|---|---|
| `PiServerOperationErrorCode` | `PiServerOperationErrorCode` with `as_str`/`protocol_code` | `tests/protocol.rs`, `tests/conformance.rs` error-code assertions |
| `INTERNAL_SERVER_ERROR_MESSAGE`, `NOT_IMPLEMENTED_MESSAGE` | same consts | `tests/conformance.rs::{does_not_expose_unexpected_service_errors_to_clients, keeps_not_implemented_stable}` |
| `PiServerError` class | `PiServerError` struct (`code`, `message`, `details`) | `tests/protocol.rs::bridge_errors_are_displayable_errors`, conformance |
| `SessionBusyError` | `PiServerError::session_busy` | `tests/sessions.rs` busy/lock tests |
| `SessionLockedError` | `PiServerError::session_locked` | `tests/sessions.rs::maps_service_lock_errors_and_rejects_control_from_unattached_clients` |
| `SessionNotFoundError` | `PiServerError::session_not_found` | conformance attach/error paths |
| `NotImplementedError` | `PiServerError::not_implemented` | `tests/conformance.rs::keeps_not_implemented_stable` |
| `InternalServerError` (cause retained, never serialized) | `InternalServerError` with boxed cause | `tests/conformance.rs::reports_wrapped_internal_causes_without_exposing_them` |

## Branches
- `toProtocolError` in `server.ts` maps `InternalServerError` → reported cause + `internal_error`; `PiServerError` → its code (with `not_implemented` message fixed); `ProtocolValidationError` → `invalid_request`; everything else → reported + `internal_error`. Covered by the conformance tests above.

## Gaps / exclusions
- TypeScript subclass identity is replaced by constructors on one typed error struct; no Rust code inspects subclass names.
- Details carry `pi_protocol::JsonValue`, the typed protocol subset; JavaScript-only values are excluded at the boundary (see `server-protocol.md`).
