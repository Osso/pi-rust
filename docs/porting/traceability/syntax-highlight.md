# Coding-agent syntax-highlight traceability

Pinned source: `packages/coding-agent/src/utils/syntax-highlight.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`, with `highlight.js@10.7.3` from the pinned `node_modules`.

Rust: `pi_coding_agent::utils::syntax_highlight` (`mod.rs`), plus a private engine (`hljs.rs`), a JavaScript-regex translator (`js_regex.rs`), and a generated grammar IR (`grammars.json`, produced by `oracle/hljs-grammar-gen.mjs` from the pinned package).

Status: **partial**. `renderHighlightedHtml` and the option/theme contract are complete and differential-tested. The highlight.js engine is ported and differential-tested for a verified language set; the remaining grammars are explicit gaps listed below.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `syntax-highlight.ts:1-13` | `HighlightFormatter`, `HighlightTheme`, `HighlightOptions` | `HighlightFormatter`, `HighlightTheme`, `HighlightOptions` |
| `syntax-highlight.ts:15-16` | `SPAN_CLOSE`, `HIGHLIGHT_CLASS_PREFIX` | Same constants |
| `syntax-highlight.ts:18-37` | `getScopeFromSpanTag` (`/\sclass\s*=\s*(?:"..."\|'...')/`, first `hljs-` token) | `scope_from_span_tag`/`class_attribute`; focused single/double-quoted and differential render cases |
| `syntax-highlight.ts:39-66` | `getScopeFormatter`: exact, then `.` prefix, then `-` prefix | `scope_formatter`; differential render cases |
| `syntax-highlight.ts:68-81` | `getActiveFormatter`: innermost mapped scope, else `default` | `active_formatter`; focused nested-scope tests |
| `syntax-highlight.ts:83-90` | Span open tag detection (`<span` + `>`, space, tab, LF, CR) | `is_span_open_tag_start`; differential render cases |
| `syntax-highlight.ts:92-132` | `renderHighlightedHtml`: span stack, entity decoding, text buffering | `render_highlighted_html`; focused entity/inheritance tests plus 14 differential render cases |
| `syntax-highlight.ts:134-142` | `highlight`: `hljs.highlight` with `ignoreIllegals`, else `highlightAuto(code, languageSubset)` | `highlight`; differential `snippet-*`, `tricky-*`, `auto-*` cases |
| `syntax-highlight.ts:144-146` | `supportsLanguage` = `hljs.getLanguage(name) !== undefined` | `supports_language`; focused and differential `supports-*` cases |
| `hljs core: compileLanguage` | `beginKeywords`, `match`, `illegal` arrays, default `begin`/`end`, `terminatorEnd` composition, `endSameAsBegin`, keywords (`$pattern`, `lexemes`, class maps), `variants` expansion, `self`, `starts`, mode cycles | `compile_mode`, `expand_contains`, `merge_variant`, `compile_keywords`, `build_rules`; differential corpus |
| `hljs core: highlight` | Mode stack, `skip`, `excludeBegin/End`, `returnBegin/End`, `endsWithParent`, `endsParent`, sublanguage continuations, keyword emitter, illegal handling, zero-width guard, safe-mode fallback | `Highlighter` (`run`, `process_lexeme`, `do_begin_match`, `do_end_match`, `process_keywords`, `process_sub_language`); differential corpus |
| `hljs core: highlightAuto` | Relevance sort, superset tie-break, plaintext baseline | `highlight_auto_internal`; differential `auto-*` cases |
| `hljs core: SHEBANG`/`END_SAME_AS_BEGIN`/JSX callbacks | `on:begin`/`on:end` behaviors used by grammars | `callback_ignores` (`shebang`, `end_same_begin`, `end_same_end`, `skip_if_preceding_dot`, `jsx_truly_opening_tag`) |

Differential: `oracle/coding-agent-syntax-highlight-oracle.ts` calls the real pinned module (request `27006`). It compares full rendered HTML for 7 registered languages (large snippets plus 36 tricky inputs), 14 raw-HTML render cases, and language-name lookups. Registered set: `bash`, `diff`, `java`, `json`, `plaintext`, `rust`, `yaml`.

## Registered-language coverage

Each registered language is exercised by a realistic multi-construct snippet under both `ignoreIllegals` values and by the tricky corpus (unterminated strings/comments, HTML-significant characters, numbers, Unicode, CRLF, empty input, escapes).

## Gaps (explicit)

- `typescript`, `javascript`: engine output matches, but the first compile takes ~12 s because fancy-regex compiles the grammar's large rule set slowly. Unregistered to avoid freezing the TUI.
- `python`: same compile-cost gap (~4 s).
- `markdown`: the compiler does not terminate on its variant/`self` graph; unregistered.
- `c`, `cpp`, `css`, `sql`, `xml`: differential mismatches in tag/attribute or literal modes; unregistered pending fixes.
- The remaining ~170 grammars in `grammars.json` (elixir, go, ruby, php, kotlin, swift, lua, scala, haskell, perl, ...) are serialized but not registered and not verified.
- `highlightAuto` without a subset ranks only registered languages, so detection differs from the source when the true best language is unregistered.

## Deviations

- JavaScript regex constructs are translated to `fancy-regex`: `\d`/`\w`/`\s` become their ECMAScript ASCII/space sets, `.` excludes `\n\r\u2028\u2029`, `^`/`$` become line-terminator lookarounds, and `\b`/`\B` become explicit ASCII word-boundary lookarounds (fancy-regex has no non-Unicode flag). `\uXXXX` becomes `\u{XXXX}`.
- `language.case_insensitive` compiles with `(?i)` (Unicode folding); JavaScript without `u` folds ASCII only.
- `highlight()` for an unregistered language returns HTML-escaped source, matching the source's `highlightCode` fallback rather than throwing.
- Grammars are compiled lazily and cached per process; the first use of a registered language pays the compile cost (≤ ~1.7 s for `yaml`).
