# Coding-agent syntax-highlight traceability

Pinned source: `packages/coding-agent/src/utils/syntax-highlight.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`, with `highlight.js@10.7.3` from the pinned `node_modules`.

Rust: `pi_coding_agent::utils::syntax_highlight` (`mod.rs`), plus a private engine (`hljs.rs`), a JavaScript-regex translator (`js_regex.rs`), and a generated grammar IR (`grammars.json`, produced by `oracle/hljs-grammar-gen.mjs` from the pinned package).

Status: **complete**. `renderHighlightedHtml` and the option/theme contract are differential-tested, and every grammar generated from the pinned `highlight.js` package (189 keys, including aliases) is differential-tested against the real engine.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `syntax-highlight.ts:1-13` | `HighlightFormatter`, `HighlightTheme`, `HighlightOptions` | `HighlightFormatter`, `HighlightTheme`, `HighlightOptions` |
| `syntax-highlight.ts:15-16` | `SPAN_CLOSE`, `HIGHLIGHT_CLASS_PREFIX` | Same constants |
| `syntax-highlight.ts:18-37` | `getScopeFromSpanTag` (`/\sclass\s*=\s*(?:"..."\|'...')/`, first `hljs-` token) | `scope_from_span_tag`/`class_attribute`; focused single/double-quoted and differential render cases |
| `syntax-highlight.ts:39-66` | `getScopeFormatter`: exact, then `.` prefix, then `-` prefix | `scope_formatter`; differential render cases. A scope named `constructor` reads `Object.prototype.constructor` in the source; `theme_formatter` preserves the resulting identity formatting (see Deviations). |
| `syntax-highlight.ts:68-81` | `getActiveFormatter`: innermost mapped scope, else `default` | `active_formatter`; focused nested-scope tests |
| `syntax-highlight.ts:83-90` | Span open tag detection (`<span` + `>`, space, tab, LF, CR) | `is_span_open_tag_start`; differential render cases |
| `syntax-highlight.ts:92-132` | `renderHighlightedHtml`: span stack, entity decoding, text buffering | `render_highlighted_html`; focused entity/inheritance tests plus 14 differential render cases |
| `syntax-highlight.ts:134-142` | `highlight`: `hljs.highlight` with `ignoreIllegals`, else `highlightAuto(code, languageSubset)` | `highlight`; differential `snippet-*`, `tricky-*`, `auto-*` cases |
| `syntax-highlight.ts:144-146` | `supportsLanguage` = `hljs.getLanguage(name) !== undefined` | `supports_language`; focused and differential `supports-*` cases |
| `hljs core: compileLanguage` | `beginKeywords`, `match`, `illegal` arrays, default `begin`/`end`, `terminatorEnd` composition, `endSameAsBegin`, keywords (`$pattern`, `lexemes`, class maps), `variants` expansion, `self`, `starts`, mode cycles | `compile_mode`, `expand_contains`, `merge_variant`, `compile_keywords`, `build_rules`; differential corpus |
| `hljs core: highlight` | Mode stack, `skip`, `excludeBegin/End`, `returnBegin/End`, `endsWithParent`, `endsParent`, sublanguage continuations, keyword emitter, illegal handling, zero-width guard, safe-mode fallback | `Highlighter` (`run`, `process_lexeme`, `do_begin_match`, `do_end_match`, `process_keywords`, `process_sub_language`); differential corpus |
| `hljs core: highlightAuto` | Relevance sort, superset tie-break, plaintext baseline | `highlight_auto_internal`; differential `auto-*` cases |
| `hljs core: SHEBANG`/`END_SAME_AS_BEGIN`/JSX callbacks | `on:begin`/`on:end` behaviors used by grammars | `callback_ignores` (`shebang`, `end_same_begin`, `end_same_end`, `skip_if_preceding_dot`, `jsx_truly_opening_tag`) |

Differential: `oracle/coding-agent-syntax-highlight-oracle.ts` calls the real pinned module (request `27006`). It compares full rendered HTML for 7 representative languages (large snippets plus 36 tricky inputs), 14 raw-HTML render cases, language-name lookups, and a themed case for every generated grammar. `crates/pi-coding-agent/tests/differential_syntax_highlight_grammars.rs` (request `27007`) compares the raw highlight.js HTML byte-for-byte for all 189 grammar keys against the pinned `highlight.js` engine using one multi-construct snippet.

## Registered-language coverage

Every grammar key and alias in `grammars.json` resolves through `supports_language`, and the raw differential exercises all 189 keys. The themed differential additionally exercises each grammar through `renderHighlightedHtml`. The 7 representative languages (`bash`, `diff`, `java`, `json`, `plaintext`, `rust`, `yaml`) also run realistic multi-construct snippets under both `ignoreIllegals` values and the tricky corpus (unterminated strings/comments, HTML-significant characters, numbers, Unicode, CRLF, empty input, escapes).

## Gaps (explicit)

- None for grammar resolution or raw highlighting.

## Discrepancies

- The TypeScript theme lookup reads `theme[scope]` from a plain object, so a scope named `constructor` resolves to `Object.prototype.constructor`. Calling it with a string returns that string unchanged, suppressing both the scope formatter and `default`. `theme_formatter` preserves this behavior with an identity formatter so ReasonML constructors match byte-for-byte. Classified as a source bug preserved for parity; a future source fix should remove the shim.
- The zero-width begin/end guard advances one Unicode scalar in Rust, while highlight.js advances one UTF-16 code unit. This differs only when a zero-width rule fires on an astral-plane character; no grammar in the pinned set does so in the differential corpus.

## Deviations

- JavaScript regex constructs are translated to Rust regex syntax: `\d`/`\w`/`\s` become their ECMAScript ASCII/space sets, `.` excludes `\n\r\u2028\u2029`, `^`/`$` become line-terminator lookarounds, and `\b`/`\B` become explicit ASCII word-boundary lookarounds. `\uXXXX` becomes `\u{XXXX}`. Inside character classes, `[`, `&`, and `~` are escaped because Rust treats them as set operators while JavaScript treats them literally.
- Patterns compile with `regex` when they contain no lookaround/backreference and fall back to `fancy-regex` otherwise. All patterns carry the multiline flag, matching highlight.js's `m` flag.
- `language.case_insensitive` compiles with `(?im)` (Unicode folding); JavaScript without `u` folds ASCII only.
- `highlight()` for an unregistered language returns HTML-escaped source, matching the source's `highlightCode` fallback rather than throwing.
- Grammars are compiled lazily and cached per process; the first use of a language pays its compile cost.
