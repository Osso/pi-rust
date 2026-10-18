# errors traceability

Pinned source: `packages/client/src/errors.ts`. Rust: `crates/pi-client/src/errors.rs`.

| TypeScript export | Observable contract | Rust mapping and proof |
|---|---|---|
| `PiServerError` | Carries protocol `code`, `message`, `details` | `PiServerError::{new, code, message, details}`; `connection::rejects_typed_handshake_version_error`, `requests::surfaces_typed_request_errors` |
| `PiDisconnectedError(message = "Pi client is disconnected")` | Default and explicit messages | `PiDisconnectedError::{new, message}`, `Default`; transport-error and close tests |
| `PiClientDisposedError` | Fixed `"Pi client is disposed"` | `PiClientDisposedError` unit struct; `disposal::disconnects_invalidates...` |
| `PiSessionOwnershipError(sessionId, message)` | Session id plus message | `PiSessionOwnershipError::{new, session_id, message}`; `sessions::enforces_exclusive_and_shared_lease_modes` |
| `PiSessionDetachedError(sessionId)` | Message `Session <id> is not attached` | `PiSessionDetachedError::{new, session_id}` with exact `Display`; session detach/abort tests |
| `toError(error)` | Non-Error values become `Error(String(value))` | Not exported: Rust has no untyped thrown values. Transport/panic sources are converted at their typed boundary. |
| `toDisconnectedError(error)` | Preserves an existing disconnected error, otherwise wraps the message | `to_disconnected_error(PiError) -> PiError`; transport-error test asserts the preserved message |

## Rust boundary

- Public operations return `Result<_, PiError>`; `PiError` is a cloneable enum with `Server`, `Disconnected`, `Disposed`, `SessionOwnership`, `SessionDetached`, `Protocol`, `Transport`, `Io`, `InvalidArgument`, and `Listener` variants.
- `PiError::raw_os_error()` preserves Unix connect errno (`unix::rejects_connection_errors`).

## Gaps / discrepancies

- `PiError::message()` returns an owned `String` mirroring the JavaScript `Error.message` value for every variant.
- Listener "throws" are Rust panics caught at the callback boundary and reported as `PiError::Listener`.
