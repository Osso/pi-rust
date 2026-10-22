# Coding-agent syntax-highlight — Q18 native UTF-16

Frozen source: `packages/coding-agent/src/utils/syntax-highlight.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, `highlight.js@10.7.3`.

`mod.rs` owns the native formatter/options/error API and HTML renderer; `hljs.rs` interprets generated grammars over `JsString` and shared `pi_js_compat::js_regex::JsRegex`. The scalar Plain/Fancy engines and their regex translation module are removed, not retained as alternate paths. Immutable generated grammar metadata remains scalar; runtime code, captures, buffers, dynamic regexes, keywords and emitted HTML retain UTF-16 units without encoding.

| Frozen behavior | Native mapping | Proof |
| --- | --- | --- |
| highlight with a named language or alias | `highlight` / `highlight_html` | existing themed differential; all 189 generated grammar keys, raw HTML differential |
| empty/missing language selects auto | `HighlightOptions.language` | raw-unit differential with plaintext subset |
| nonempty unknown language throws Error | `Result<JsString, HighlightError>`; exact `message: JsString` | unknown scalar/raw language names; no escaped-text fallback or panic |
| registered name/alias lookup | `supports_language(&JsString)` | existing supports cases, raw unknown names |
| legacy UTF-16 RegExp with `m` / `mi` | shared `JsRegex` directly | all-grammar differential; raw code in 19 languages/aliases and 5-language insertion matrix |
| text buffering, captures, zero-width advance | native `JsString` slices and single-unit advancement | lone high/low units, valid pairs, replacement character, mixed pairs; no scalar roundtrip |
| span stack, class extraction, formatter inheritance, dot/dash prefixes | `render_highlighted_html` | existing render differential, raw text/scopes, JS whitespace in attributes |
| inherited `constructor` scope identity | native identity formatter | existing ReasonML corpus and raw HTML scope case; preserved source behavior |
| HTML entities and consumed lengths | shared [HTML decoder](html.md) | numeric surrogate/astral values, unit offsets and raw entity suffixes |

## Grammar integrity and performance

Regeneration from the current common pin produces byte-identical `grammars.json`: SHA256 `19d873f14576410c388f4d6518ca1fcddb37c6b3d2d4d5839a9415c9520742ac`. Guarded generator proof: `/tmp/pi-rust-repin-lossless-js-libs/highlight-grammar-integrity.log`.

Debug-build first compile+match times on the same multi-construct snippet, grammar order and host (not a benchmark guarantee):

| Grammar | Shared native regex | Previous Plain/Fancy engine |
| --- | ---: | ---: |
| Swift | 78.34 ms | 405.07 ms |
| Gauss | 174.31 ms | 3416.34 ms |
| PostgreSQL | 247.21 ms | 1643.09 ms |
| PowerShell | 107.16 ms | 247.09 ms |
| SAS | 91.63 ms | 65.11 ms |

New all-189 differential: 4.63 s including TS oracle; previous engine's native all-189 loop: 16.05 s. SAS's bounded slowdown is reported, not hidden behind an alternate matcher.

## Proof ledger and boundaries

`/tmp/pi-rust-repin-lossless-js-libs/` contains `red-highlight.log` (old scalar API replaces a TS-created lone surrogate), `green-highlight-initial.log`, `green-highlight-expanded.log`, `highlight-development-suites1.log` (23 tests, including the existing all-189 and themed differentials), and `highlight-baseline-wall-time.log` (temporary old-engine benchmark). All runtime proofs use the mandatory runner; standalone harness uses the actual worktree sources because unrelated coding-agent lanes block the crate. `green-highlight-final-suites.log` covers the final source (24 tests across six dedicated suites, including direct raw HTML/entity APIs and all 189 generated grammars); all PASS, with no compiler warnings. The frozen oracle intentionally logs three unknown-language diagnostics to stderr.

Behavioral mutants `mutant-highlight-replace-surrogates.log`, `mutant-html-reject-surrogate.log`, `mutant-unknown-language-message.log` all fail runtime differential assertions. Restored before commit. No lint, independent integration, or Q18 acceptance claim is made here.

## Explicit gaps

- The pre-existing grammar generator excludes **Mathematica** (`SYSTEM_SYMBOLS_SET` callback) and **R** (`beforeMatch` compiler callback). Frozen regeneration reports both; 189 serialized grammars are not the complete highlight.js registry. These are explicit **unapproved pre-existing gaps**. Parent instruction: record them without widening this UTF-16 slice.
- External caller adaptation and full coding-agent compilation belong to parent integration. The caller is `modes/interactive/theme/mod.rs`; both highlight functions now return `Result`.
- This bounded corpus proves the listed behaviors, not every possible grammar/input combination.
