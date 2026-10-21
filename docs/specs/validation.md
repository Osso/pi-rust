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

## Current proof boundary

This re-pin repairs optional-null normalization and declared union-arm coercion
order. It does not establish whole-module parity. The current-pin nullable-array
compiler throw, nested TypeBox metadata representation, non-object runtime input,
JavaScript regular expressions, and other engine limitations remain explicit
unresolved boundaries in the traceability document; none is newly waived here.
