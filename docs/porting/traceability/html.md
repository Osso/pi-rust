# coding-agent html traceability

Pinned source: `packages/coding-agent/src/utils/html.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `26002` executes the real pinned module across 31 named-entity, numeric, boundary, and position cases. Focused tests: `crates/pi-coding-agent/tests/html.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `html.ts:1-4` | Decoded entity exposes `text` and consumed `length` | `utils::html::DecodedHtmlEntity`; focused `decodes_entity_at_index_with_consumed_length` |
| `html.ts:6-11` | Reject non-integer, negative, and above-`0x10ffff` code points | `decode_code_point` uses `u32::try_from` and `char::from_u32`; focused `rejects_out_of_range_code_points` |
| `html.ts:13-25` | Five named entities, case-sensitive | `decode_html_entity`; focused `decodes_named_entities`, `rejects_unknown_named_entities` |
| `html.ts:27-29` | `#x`/`#X` hexadecimal `parseInt` | `parse_int` with radix 16 and `0x` prefix handling; focused `follows_javascript_parse_int_prefix_rules` |
| `html.ts:31-33` | `#` decimal `parseInt` | `parse_int` with radix 10; same focused test |
| `html.ts:38-41` | Semicolon search starts at `index + 1`; reject when missing or more than 16 characters away | `decode_html_entity_at`; focused `rejects_entities_without_semicolon_or_beyond_16_characters`, `accepts_the_exact_16_character_boundary` |
| `html.ts:44-50` | Decode entity slice and return `length = semicolonIndex - index + 1` | `decode_html_entity_at`; focused length cases |

## Discrepancies

- `String.fromCodePoint(0xD800..=0xDFFF)` produces lone surrogates in JavaScript. Rust `String` cannot represent them, so surrogate references return `None` (typed-boundary exclusion).
- JavaScript indexes strings by UTF-16 code units; the Rust API takes a byte offset. ASCII prefixes are identical; non-ASCII offsets are an explicit typed-boundary deviation.

## Proof inventory

- Focused cases: named, numeric, malformed, boundary, and position behavior.
- Differential request: `26002`, 31 cases (surrogates and non-ASCII offsets excluded).
