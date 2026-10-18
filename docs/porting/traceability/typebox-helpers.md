# utils/typebox-helpers traceability

Pinned source: `packages/ai/src/utils/typebox-helpers.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/utils/typebox_helpers.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `StringEnum(values, options?)` | Builds a TypeBox `Type.Unsafe` schema `{ type: "string", enum: values }` with `description` and `default` only when the option is truthy (empty strings are omitted). | `string_enum(&[&str], StringEnumOptions) -> TypeBoxSchema`. Test: `string_enum_builds_a_google_compatible_string_schema` (exact JSON, insertion order, empty-option omission) plus the `typebox-unsafe-string-enum` differential case. |
| `TUnsafe<T[number]>` return type | The schema is a TypeBox schema, so downstream `Value.Convert` treats it as TypeBox origin. | `TypeBoxSchema` newtype; `Tool::typebox` sets `Tool::typebox_kind`. See [validation traceability](validation.md) for the origin representation. |

## Representation decisions

- TypeBox origin is not representable on a plain `serde_json::Value`. `StringEnum` therefore returns
  `TypeBoxSchema`, and the only way to attach it to a `Tool` is `Tool::typebox`, which records the
  origin. Plain JSON schemas use `Tool::new`.
- The helper emits the same key order as the TypeScript object literal (`type`, `enum`,
  `description`, `default`).

## Gaps

- None. The module has no other exports or branches.

## Discrepancies

- TypeBox 1.3.7's `Type.Unsafe` marks nodes with a hidden `~unsafe` property and no `~kind`, so
  `Value.Convert` skips `StringEnum` nodes in TypeScript. Rust records origin per tool, so a
  `StringEnum` node inside a TypeBox tool is converted by the TypeBox pass. For `type: "string"` the
  observable result is identical (the following plain-schema coercion produces the same values and
  messages); see [validation traceability](validation.md#gaps).
