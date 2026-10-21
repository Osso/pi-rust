# Shared tool argument validation

Source: `packages/ai/src/utils/validation.ts` at
`4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Trace: [validation](../porting/traceability/validation.md).

## Contract

- `validateToolCall` selects the first tool with an equal name; missing tools report
  `Tool "<name>" not found`. Validation uses the call name in errors.
- Validation clones arguments. Explicit optional null properties are not deleted.
  Source calls `Value.Convert` on the clone and discards its root replacement
  return, then obtains/compiles the validator before plain-schema coercion.
  Conversion dispatch depends on per-node hidden kinds, not JSON `type` alone.
- Plain-schema coercion runs only for an object schema without the root
  `Symbol.for("TypeBox.Kind")` marker. It follows `allOf`, then `anyOf`, then `oneOf`, then declared
  types, object properties/additional properties, and array items.
- Each `anyOf`/`oneOf` candidate starts from a fresh clone, is coerced and checked
  in declaration order. An already-valid later arm does not bypass an earlier
  coercible arm. If no candidate validates, the original value is unchanged.
- TypeBox string conversion turns null into `"null"`; plain string coercion turns
  null into `""`. Enum validation follows conversion/coercion. Invalid optional
  enum values are errors, not absent properties.
- Validation errors retain source ordering, paths, messages, and original received
  arguments. Caller-owned arguments remain unchanged on success and failure.

## Lossless value boundary

- `validate_arguments(&JsValue schema, &JsValue arguments, bool typebox_kind,
  &str tool_name)` is the validation/coercion authority and returns `JsValue`.
  Arguments, object keys, schema string literals, issue paths and error messages
  retain UTF-16 units. Argument storage, coercion and output never round-trip through
  serde values or replace unmatched surrogates. Temporary frozen URL/hash encoding
  behavior is explicit below.
- Tool entrypoints return `JsValue`; checkpoint `ToolCall` still has scalar JSON
  storage and adapts losslessly into this authority. The content lane replaces that
  input construction once `ToolCall.arguments` becomes `JsObject`.
- `schema_error_lines(&JsValue, &JsValue)` returns `Vec<JsString>`.
  `ValidationError::message()` returns exact `&JsString`; `Display` shows scalar
  messages directly and JSON-escapes messages containing unmatched surrogates.
- RegExp preserves frozen flags: `pattern`/`patternProperties` use Unicode mode;
  conversion/additional-properties patterns use non-Unicode UTF-16-unit mode. Length checks retain
  the frozen TypeBox grapheme algorithm, not Rust scalar or byte length.

## Current proof boundary

This re-pin repairs optional-null normalization and declared union-arm coercion
order. It does not establish whole-module parity. Nullable-array handling follows
its approved guarded-source delta. Nested TypeBox metadata representation and other
engine limitations remain explicit in the traceability document. Direct core calls with non-object arguments or primitive schemas are
outside the standing typed-core input policy: frozen `ToolCall.arguments` is
`Record<string, any>` and `Tool.parameters` extends `TSchema` (`types.ts:378,478–481`).
This does not exclude raw persisted JSON or valid nested JSON values.

## Frozen format and hashing behavior

The default TypeBox format registry is enforced, including unknown-format success,
Unicode/IDN hostnames, dates/times, IPs, pointers, URI/IRI, and ECMAScript regex formats.
Unmatched surrogates stay intact through NFC normalization. WHATWG URL checks and
reference resolution convert temporary parse input to USVString as the frozen
constructor does; unmatched-surrogate anchors can consequently alias.

`uniqueItems` uses frozen FNV-1a hashing, not structural equality. Its TextEncoder
step replaces unmatched surrogates: distinct lone-surrogate strings can therefore
hash identically and count as duplicates. This source quirk is deliberately preserved
without modifying stored arguments; object keys are sorted by UTF-16 for hashing.
