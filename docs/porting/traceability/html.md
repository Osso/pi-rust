# Coding-agent HTML — Q18 UTF-16

Frozen source: `packages/coding-agent/src/utils/html.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. Implementation: `utils/html.rs`; [highlighting integration](syntax-highlight.md).

| Source contract | Native behavior / proof |
| --- | --- |
| `DecodedHtmlEntity.text`, `length` | one `JsString` authority, UTF-16 consumed units |
| five named entities, case-sensitive | `decode_html_entity(&JsString)`; existing unit/differential cases |
| `Number.parseInt` decimal/hex prefix behavior | sole shared `pi_js_compat::number::parse_int` over native units; signs, JS whitespace, partial digits, optional hex prefix, negative zero |
| `String.fromCodePoint`, including surrogate values | retain single surrogate units and valid astral pairs; reject negative/above `0x10ffff` |
| semicolon search at `index + 1`, maximum distance 16 | `decode_html_entity_at(&JsString, usize)`; existing 16-unit boundary and raw prefix tests |
| indexed helper does not validate the leading ampersand | direct frozen differential `xamp;`, index 0 |

Tests: `tests/html.rs`, `tests/differential_html.rs` (existing 31 cases), `tests/lossless_syntax_highlight.rs` (direct raw entity API, indexed raw prefixes, highlighted HTML). Oracle: `oracle/lossless-highlight-oracle.mjs` imports the unchanged frozen HTML module and reports unit arrays rather than decoding through scalar JSON.

The former surrogate rejection and byte-offset deviations are removed. No second scalar decoder remains. Only production consumer is the syntax highlighter. Typed API takes nonnegative integer indices; invalid JS runtime argument types remain outside its scope.

Runtime proof uses the same protected standalone harness as [highlighting](syntax-highlight.md); parent owns full-crate integration and independent checks.
