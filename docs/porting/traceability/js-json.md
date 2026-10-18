# js_json traceability (support module)

Rust: `crates/pi-coding-agent/src/utils/js_json.rs`.
Source contract: the JavaScript engine's own `JSON.parse` / `JSON.stringify` (V8 as shipped by the
pinned Node runtime), required because persistent agent files are shared byte-for-byte with the
TypeScript runtime and their read errors embed V8 messages.

Differential proof: `oracle/js-json-oracle.ts` runs the engine's built-ins over 1617 inputs
(`crates/pi-coding-agent/tests/differential_js_json.rs`): 11 seed documents plus every truncation,
single-character deletion, and insertion of 26 structural characters at every position, plus
whole-source special values. Comparisons are byte-for-byte on `JSON.stringify(value)`,
`JSON.stringify(value, null, 2)`, and the `SyntaxError.message`. Focused tests:
`crates/pi-coding-agent/tests/js_json.rs` (6 cases).

| Observable contract | Rust mapping and proof |
|---|---|
| `JSON.parse` accepts JSON grammar with only space/tab/LF/CR as whitespace | `Parser`; differential mutations cover BOM, NBSP, control characters |
| Duplicate keys keep the first position and the last value | `Map::insert`; `parse_keeps_last_duplicate_value_at_first_position` |
| Numbers become IEEE-754 doubles; `-0` prints as `0`; `1e400` becomes `null` on output | `number_value` + `pi_js_compat::number::js_number_to_string`; `numbers_round_trip_as_javascript_doubles`, differential seeds |
| Object keys: array-index keys ascending first, then insertion order | `object_entries`; `stringify_orders_array_index_keys_first_like_javascript_objects` |
| `JSON.stringify` escapes `"`, `\`, `\b\f\n\r\t`, other C0 as `\u00xx` | `write_string`; `pretty_output_matches_json_stringify_with_two_space_indent` |
| Pretty output uses two spaces per level and `": "` separators | `stringify_pretty`; same test |
| `SyntaxError` templates: end of input, dedicated messages with `at position N (line L column C)`, `Unexpected token 'x', ...` with 10-unit context | `unexpected_token`, `unexpected_token_with_context`, `error_at`; `parse_errors_use_v8_messages_with_positions` and the differential suite |
| Positions and line/column count UTF-16 units; `\r\n`, `\r`, `\n` each end a line | `utf16_position`, `line_and_column`; differential cases with CR-only and CRLF inputs |
| Whole-source `NaN`/`Infinity`/`undefined`/`[object Object]` report `"x" is not valid JSON` | `SPECIAL_SOURCES`; differential suite |
| Escapes: `\uXXXX`, surrogate pairs, `\/` | `parse_unicode_escape`; `string_escapes_decode_surrogate_pairs` |

## Discrepancies

- JavaScript strings can hold lone surrogates; Rust strings cannot. Lone surrogate escapes decode to
  U+FFFD, and a non-BMP character reported as V8's single-unit token also prints as U+FFFD. UTF-8
  encoding of a lone surrogate produces U+FFFD in Node as well, so file and stdout bytes match; the
  oracle applies `String.prototype.toWellFormed()` for the comparison.
- Integers above 2^53 are stored exactly by `serde_json` but lose precision in JavaScript. Values read
  from JavaScript-written files are already rounded, so round-trips match; a Rust writer emitting an
  integer above 2^53 would print the exact value where JavaScript prints the rounded one. No writer in
  this port produces such values (timestamps and costs stay below the limit).

## Proof inventory

- Focused cases: 6.
- Differential cases: 1617.
