# markdown.ts traceability

Source: `packages/tui/src/components/markdown.ts` (1027 lines) plus the pinned
`marked` 18.0.5 lexer (`node_modules/marked`, `src/rules.ts`, `src/Tokenizer.ts`,
`src/Lexer.ts`, `src/helpers.ts`).

Rust: `crates/pi-tui/src/components/markdown/{mod.rs,lexer.rs,tokens.rs}`.

Tests: `crates/pi-tui/tests/markdown.rs` (81 ported TS cases + a theme wiring guard),
`crates/pi-tui/tests/differential_markdown.rs` (749-case lexer corpus and
1,739 rendered cases), `crates/pi-tui/tests/markdown_isimageline_regression.rs`
(10 ported bug-regression cases; the source's "old implementation" simulation
tests a local lambda rather than production code and is replaced by the
`markdown_does_not_wrap_image_lines` integration case).

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `interface DefaultTextStyle` | `DefaultTextStyle` (`StyleFn` aliases for `color`/`bgColor`) | `preserves_gray_italic_*`, differential `style-*` cases |
| `interface MarkdownTheme` | `MarkdownTheme` trait (14 required methods, optional `highlightCode`/`codeBlockIndent`) | `theme_is_used_for_all_default_styles` |
| `interface MarkdownOptions` | `MarkdownOptions` (`renderLatex` defaults to `true`) | `allows_latex_rendering_to_be_disabled`, `preserves_source_list_markers_when_configured`, differential `options-*` |
| `class Markdown` constructor | `Markdown::new(text, padding_x, padding_y, theme, default_text_style, options)` | all component tests |
| `setText(text)` | `Markdown::set_text` | `switches_from_raw_to_rendered_math_when_streamed_delimiter_closes`, transform cache test |
| `invalidate()` | `Component::invalidate` | transform cache test |
| `render(width)` | `Component::render` | all component + differential tests |
| `isHtmlCommentOnly` (private) | `is_html_comment_only` | `hides_html_comments`, differential HTML corpus |
| `trimPartialClosingFences` (private) | `trim_partial_closing_fences` (public for the differential test) | `stabilizes_partial_closing_fence_rendering`, differential streaming cases |
| `markdownParser` + `StrictStrikethroughTokenizer` | `lexer::lex` with `Rules::strict_del` | `renders_strikethrough`, `keeps_single_tilde_as_plain_text`, differential emphasis corpus |
| `LATEX_MARKDOWN_EXTENSIONS` | `tokenize_inline_latex` / `tokenize_block_latex` + start clipping | LaTeX tests, differential LaTeX corpus |
| `marked` token stream | `tokens::Token`, `ListItem`, `TableCell`, `ListStart`, `Align`, `LinkDefinition` | `differential_markdown::lex_matches_marked_token_stream` |

## Lexer branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| `Lexer.lex` CRLF/CR normalization | `lex` (`carriage_return`) | corpus `a\r\nb`, `a\rb` |
| extension tokenizers before space/inline text | `block_tokens` / `inline_tokens` LaTeX calls | LaTeX corpus |
| `space` (single `\n` appended to previous raw) | `Lexer::space` | differential whitespace cases |
| `code` (indented, merge into paragraph/text) | `Lexer::code` + `merge_block` | indented-code corpus |
| `fences` (`indentCodeCompensation`, lang punctuation unescape) | `Lexer::fences` + `indent_code_compensation` | fence corpus |
| `heading` (trailing `#`, depth) | `Lexer::heading` | heading corpus |
| `hr` | `Lexer::hr` | hr corpus |
| `blockquote` (line grouping, setext shielding, continuation into blockquote/list, top state) | `Lexer::blockquote` | blockquote corpus, `renders_list_content_inside_blockquotes` |
| `list` (item regex, indent search, nested-block breaks, loose/tight, task checkbox, marker preservation) | `Lexer::list` | 19 list tests, list corpus |
| `html` block (`pre` flag, trailing blank trim) | `Lexer::html` | HTML corpus |
| `def` (link map, merge into paragraph) | `Lexer::def` + `merge_definition` | reference-link corpus |
| `table` (`splitCells`, alignment, column-count check) | `Lexer::table` + `split_cells` | table tests, table corpus |
| `lheading` | `Lexer::lheading` | setext corpus |
| `paragraph` (extension start clipping) | `Lexer::paragraph` + `clip_block_paragraph` | paragraph corpus |
| `text` (merge consecutive) | `Lexer::text` + `merge_text` | paragraph/text corpus |
| `escape` | `Lexer::escape` | escape corpus |
| `tag` (`inLink`/`inRawBlock` state) | `Lexer::tag` | HTML corpus |
| `link` (angle brackets, paren balancing, title) | `Lexer::link` + `output_link` | link corpus |
| `reflink`/`nolink` fallback to first-char text | `Lexer::reflink` | reference-link corpus |
| `emStrong` delimiter algorithm (CommonMark rules 9–10) | `Lexer::em_strong` | emphasis corpus (~100 cases), astral emoji |
| `codespan` (newline collapse, space trim) | `Lexer::codespan` | codespan corpus |
| `br` | `Lexer::br` | line-break corpus |
| strict `del` tokenizer override | `Lexer::del` (`strict_del`) | strikethrough tests/corpus |
| `autolink` | `Lexer::autolink` | autolink corpus |
| `url` GFM + `_backpedal` | `Lexer::url` | URL corpus |
| `inlineText` (`escaped`, `prevChar`, `keepPrevChar`) | `Lexer::inline_text` + `clip_inline_text` | inline corpus |
| reflink/escape/blockSkip masking | `mask_reflinks`, `mask_global` | emphasis/link corpus |
| deferred `inlineQueue` | per-token `inline_source` resolved by `resolve_inline` | all token comparisons |
| `trimPartialClosingFences` | `trim_partial_closing_fences` | streaming fence test |
| `splitCells`, `expandTabs`, `findClosingBracket`, `rtrim`, `trimTrailingBlankLines` | same-named helpers | table/list/link corpus |
| `cachedIndentRegex` clamp (`indent-1`) quirk | `indent_regex` in `Lexer::list` | nested-list tests |
| `edit().replace()` caret stripping for inserted patterns | `strip_carets` | blockquote corpus |

## Renderer branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| cache keyed by text+width, `setText`/`invalidate` | `Markdown` cache fields + `Component` impl | transform cache test |
| `transform(markdown, contentWidth)` | `MarkdownOptions::transform` | transform cache test |
| empty text returns `[]` | `render_content` early return | differential `render-0-*` |
| tabs to 3 spaces | `render_content` | differential tab corpus |
| `isImageLine` skip wrapping | `render_content` | `markdown_does_not_wrap_image_lines`, regression file |
| heading style context (`h1` underline, `h3+` prefix) | `Token::Heading` branch | heading tests, differential |
| paragraph/text spacing rules | `Token::Paragraph` / `Token::Text` | spacing tests |
| `latexBlock` (display render, pending raw, default style) | `Token::LatexBlock` | LaTeX tests |
| code block (lang fence, indent, highlight hook, spacing) | `Token::Code` | code-block tests |
| list (bullet/task markers, continuation prefix, nested depth, loose spacing) | `render_list` + marker helpers | list tests |
| table (fallback, min/natural widths, wrapping, header bold, dividers) | `render_table` | table tests |
| blockquote (quote style prefix reapplication, width−2, border, trailing empty trim) | `Token::Blockquote` | blockquote tests |
| hr (width clamp 80) | `Token::Hr` | divider tests |
| html (comment hiding, default style) | `Token::Html` | HTML tests |
| space | `Token::Space` | spacing tests |
| default token text | `Token::has_text` fallback | differential |
| inline: latex, escape, text/nested, paragraph, strong, em, codespan, link (OSC 8 / parenthetical URL), br, del, html | `render_inline_tokens` | link tests, inline corpus |
| trailing style-prefix trimming | `render_inline_tokens` loop | heading/quote tests |
| margins, background, width padding, vertical padding | `render_content` | differential `style-*`, padding cases |
| default-style sentinel prefix | `default_style_prefix`, `style_prefix` | heading/quote styling tests |

## Differential coverage

`oracle/markdown-oracle.ts` exposes two operations:

- `markdown_lex`: runs the real `marked` 18.0.5 `lexer()` with the same strict
  `del` tokenizer and LaTeX extensions configured by `markdown.ts`, then applies
  `trimPartialClosingFences`. The oracle mirrors the parser configuration because
  `markdown.ts` does not export `markdownParser`.
- `markdown_render`: instantiates the real TypeScript `Markdown` component with
  `test-themes.ts` (chalk level 3) and returns rendered lines.

The lexer corpus has 749 inputs: 178 Pi-typical/feature cases and 571
CommonMark/GFM spec-derived examples (headings, setext, hr, blockquotes, all list
forms, fences, indented code, tables, emphasis delimiter runs, links/images/
references, autolinks, HTML blocks/inline, escapes, entities, LaTeX, Unicode
whitespace, astral characters). Every token field is compared as JSON, including
absent-vs-`null` distinctions.

The render differential compares exact strings including ANSI at widths
20/25/30/40/50/60/70/80, padding `(0,0)`, `(2,0)`, `(0,1)`, `(1,1)`, hyperlinks
on/off, `preserveOrderedListMarkers`, `preserveBackslashEscapes`, `renderLatex`
off, and gray/italic/bg-blue default styles.

## Deviations

- Regex engine: `fancy-regex` 0.16.2 replaces JavaScript `RegExp` because the
  marked grammar uses lookaround and backreferences. `translate` maps JS
  shorthands to JS semantics: `\s` includes U+FEFF and excludes U+0085, `\d`/
  `\w` are ASCII, `\b` is an ASCII word boundary, `.` excludes `\n`, `\r`,
  U+2028, U+2029, and `[\s\S]` becomes `(?s:.)`.
- JavaScript indexes strings by UTF-16 code unit. `slice_utf16`/`byte_at_utf16`
  reproduce those indices. A JS slice that would split a surrogate pair cannot
  be represented by a Rust `String`; the port advances to the next scalar
  boundary. No corpus case reaches that split.
- `js_trim`/`js_trim_end` use the ECMAScript whitespace set (FEFF included,
  NEL excluded), matching `String.prototype.trim`/`trimEnd`.
- Marked's deferred `inlineQueue` is represented by a per-token `inline_source`
  resolved after the block pass. Discarded queue entries from replaced
  blockquote/list tokens are not processed; their output was discarded in the
  source as well. The task-checkbox backward queue search is approximated by a
  reverse document-order search inside the item, which is the same entry for
  every corpus case.
- Typed API exclusions (decision 2026-08-12): theme callbacks are `&str ->
  String`; JS callers passing non-string values, `transform` returning
  non-strings, and non-boolean `renderLatex` are excluded.
- `(?m)` line-start anchors (indented-code indent removal, blockquote marker
  removal) treat only `\n` as a line start; JavaScript also treats U+2028/U+2029.
  CR is normalized away; the remaining difference is limited to those two code
  points and is not exercised by the corpus.

## Discrepancies

None found in the ported source behavior. Two source quirks are preserved rather
than fixed: the `cachedIndentRegex` cache index makes the list break regexes use
`indent-1`, and `title` is absent for autolinked links but `null` for explicit
links.

## Dependencies

- `fancy-regex = 0.16.2` (exact): executes the ported marked grammar.
- `serde = 1.0.229` (exact, `derive`): serializes the token model for the
  differential oracle. `serde_json = 1.0.151` remains a dev-dependency.
