# Native Error constructor identity

Shared `NativeNodeError` must preserve constructor identity separately from mutable name/message and [AbortReason Error/DOMException branding](abort-signals.md).

## Requirements

- [x] Represent built-in Error, EvalError, RangeError, ReferenceError, SyntaxError, TypeError, URIError and AggregateError constructors explicitly.
- [x] Every built-in Error subclass is instanceof Error; subclass checks use constructor identity, never the name field.
- [x] Existing `new(name, message)` creates a generic Error; `new("TypeError", ...)` remains a renamed plain Error, not a TypeError.
- [x] Name/message/property mutation and native UTF-16 values never change constructor identity.
- [x] Node SystemError retains constructor identity, code-aware `toString`, `info`/`errno`/`syscall` data; Rust Display stays message-only. Uncaught panic formatting recognizes native errors without losing UTF-16 names/messages.
- [x] HandlerError ↔ CallbackError preserves raw-value rejections, UTF-16 object keys/text and native constructor/code data without coercing the rejection into a generic Error.
- [x] Constructor metadata is not an enumerable JS field and must not appear in error-record output. Existing DOMException branding remains distinct.
- [ ] Regular Node-internal coded errors display their captured constructor code in toString, independent of later code-property mutation. Ordinary Error objects with code properties do not gain this override; SystemError retains its distinct dynamic-code override.

## Proof and implementation

[Traceability](../porting/traceability/native-error-classes.md). Shared representation: `crates/pi-js-compat/src/node_error.rs`; Node differential: `crates/pi-js-compat/tests/native_error_classes.rs`.

Handler carrier proof: `core::extensions::types::tests::handler_error_round_trip_preserves_raw_callback_rejections`; [extension source chain](../porting/traceability/extensions-types.md#rejection-preserving-handler-carrier-2026-10-23).

## Remaining work

- [ ] Migrate native built-in error producers and instanceof-sensitive consumers to explicit constructors; this carrier change alone does not prove those consumers.
