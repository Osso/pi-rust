# utils/json-parse traceability

Pinned source: `packages/ai/src/utils/json-parse.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The partial parser mirrors `partial-json@0.1.7` (`parse` with `Allow.ALL`), the version locked by `package-lock.json`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `repairJson(json)` | Tracks string state; escapes raw `0x00-0x1f` (`\b \f \n \r \t`, else `\uXXXX`); keeps valid escapes; doubles the backslash for invalid escapes and a trailing backslash; keeps `\uXXXX` only for four hex digits. | `utils::json_parse::repair_json`; `repairs_raw_control_characters_inside_strings`, `repairs_invalid_escapes_by_doubling_the_backslash`, `preserves_valid_escapes_and_structure_outside_strings`; exact differential over 23 repair fixtures. |
| `parseJsonWithRepair(json)` | Strict `JSON.parse`; on failure parse the repaired text only when it changed, otherwise rethrow. | `parse_json_with_repair`; `parses_valid_json_and_repairs_malformed_strings`; differential compares success/failure and value for every repair fixture plus valid/malformed documents. |
| `parseStreamingJson(partialJson)` | Empty/missing/blank → `{}`; strict+repair first; then `partial-json`; then `partial-json` on repaired text; all failures → `{}`; nullish partial results → `{}`. | `parse_streaming_json`; `parses_complete_streamed_json`, `parses_incomplete_objects_arrays_strings_and_numbers`, `accepts_partial_literals_and_maps_nullish_results_to_an_object`, `returns_an_empty_object_for_missing_blank_or_unparseable_input`, `repairs_before_partial_parsing`; differential over every character prefix of nine documents (objects, arrays, nested values, strings, numbers, literals, raw control characters, emoji) plus malformed probes. |
| `partial-json` internals | Literal prefix acceptance, object/array truncation, string repair, number exponent retry, `substring` clamping/swap semantics, trailing-content tolerance. | `PartialParser` methods `parse_any`, `parse_string`, `parse_object`, `parse_array`, `parse_number`, `parse_whole_input_number`, `js_substring`. Covered by the streaming differential corpus. |

## Exclusions and deviations

- JavaScript `JSON.parse` accepts `\uD800`-style lone surrogate escapes; Rust `String`/`serde_json` reject them. Typed core APIs exclude invalid Unicode runtime input.
- JavaScript numbers are IEEE-754 doubles. Rust keeps `serde_json` number representations; differential comparison normalizes both sides to `f64`. Integers beyond 2^53 therefore stay exact in Rust while TypeScript rounds them.
- Non-finite numbers (`Infinity`, `-Infinity`, `NaN`, overflowing literals) cannot be represented in `serde_json::Value`; they map to `Value::Null`, which is the observable `JSON.stringify` output. A top-level non-finite literal is not converted to `{}` by the nullish rule.
- `parseStreamingJson` result ordering follows TypeScript insertion order because `serde_json` is built with `preserve_order`.
- No source bug found.
