# Native Error constructor identity

Shared `NativeNodeError` must preserve constructor identity separately from mutable name/message and [AbortReason Error/DOMException branding](abort-signals.md).

## Requirements

- [x] Represent built-in Error, EvalError, RangeError, ReferenceError, SyntaxError, TypeError, URIError and AggregateError constructors explicitly.
- [x] Every built-in Error subclass is instanceof Error; subclass checks use constructor identity, never the name field.
- [x] Existing `new(name, message)` creates a generic Error; `new("TypeError", ...)` remains a renamed plain Error, not a TypeError.
- [x] Name/message/property mutation and native UTF-16 values never change constructor identity.
- [x] Constructor metadata is not an enumerable JS field and must not appear in error-record output. Existing DOMException branding remains distinct.

## Proof and implementation

[Traceability](../porting/traceability/native-error-classes.md). Shared representation: `crates/pi-js-compat/src/node_error.rs`; Node differential: `crates/pi-js-compat/tests/native_error_classes.rs`.

## Remaining work

- [ ] Migrate native built-in error producers and instanceof-sensitive consumers to explicit constructors; this carrier change alone does not prove those consumers.
