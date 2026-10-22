# Lossless syntax highlighting

Frozen `packages/coding-agent/src/utils/syntax-highlight.ts` and `html.ts` at `4f9e6f8a4` define text behavior. [Implementation and proof mapping](../porting/traceability/syntax-highlight.md).

## What it must do

- [x] Preserve JS UTF-16 code units through code, language names, regex captures, HTML, formatter callbacks and return values.
- [x] Match frozen named-language/alias highlighting for all 189 serialized grammars and the existing themed corpus.
- [x] Select auto-detection for absent or empty language; retain raw subset strings without scalar conversion.
- [x] Return exact unknown-language Error messages, including raw names, through `HighlightError.message` rather than panic or fallback output.
- [x] Decode named/numeric HTML entities, including surrogate code points, and consume UTF-16 unit offsets.

## How it works

- [Highlighting traceability](../porting/traceability/syntax-highlight.md).
- [HTML traceability](../porting/traceability/html.md).

## Implementation inventory

- `crates/pi-coding-agent/src/utils/syntax_highlight/mod.rs` — native public API and HTML renderer.
- `crates/pi-coding-agent/src/utils/syntax_highlight/hljs.rs` — native grammar interpreter using the shared JS regex authority.
- `crates/pi-coding-agent/src/utils/syntax_highlight/grammars.json` — unchanged frozen grammar IR.
- `crates/pi-coding-agent/src/utils/html.rs` — shared UTF-16 entity decoder.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/{syntax_highlight,differential_syntax_highlight,differential_syntax_highlight_grammars,lossless_syntax_highlight}.rs`.
- `crates/pi-coding-agent/tests/{html,differential_html}.rs`.

## Known gaps (current cycle)

- [ ] Parent caller adaptation, independent checks and full coding-agent compilation.
- [ ] Unapproved pre-existing generator gaps: Mathematica and R callbacks; no full registry parity claim.

## Out of scope

This UTF-16 slice does not add the two previously unsupported grammar callbacks or alter unrelated caller behavior.
