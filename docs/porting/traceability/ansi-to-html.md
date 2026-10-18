# Coding-agent ANSI-to-HTML traceability

Pinned source: `packages/coding-agent/src/core/export-html/ansi-to-html.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::export_html::ansi_to_html`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `ansi-to-html.ts:14-31` | 16-entry ANSI palette | `ANSI_COLORS`; focused standard/bright tests |
| `ansi-to-html.ts:36-60` | `color256ToHex`: palette below 16, 6×6×6 cube below 232, 24 gray shades otherwise; no bounds checks | `color256_to_hex`; focused palette tests and differential cases for 0–256 plus large, huge, and infinite indices |
| `ansi-to-html.ts:65-73` | Escape `&`, `<`, `>`, `"`, `'` in that order | `push_escaped`; focused escaping test |
| `ansi-to-html.ts:75-97` | Empty style record and CSS property order `color, background-color, font-weight, opacity, font-style, text-decoration` | `TextStyle`; focused style tests |
| `ansi-to-html.ts:104-190` | SGR handling for 0–4, 22–24, 30–37, 38 (256/RGB), 39, 40–47, 48 (256/RGB), 49, 90–97, 100–107; unknown codes ignored; extended colors consume their parameters only when complete | `apply_sgr`; focused tests and differential `rgb-truncated`, `256-truncated`, `unknown` cases |
| `ansi-to-html.ts:193-237` | `/\x1b\[([\d;]*)m/g` framing: ASCII digits and semicolons only, empty parameter string means reset, text outside sequences is escaped, at most one open span | `escape_sequences` and `ansi_to_html`; focused framing tests and differential `no-terminator`/`non-sgr` cases |
| `ansi-to-html.ts:243-246` | Each line becomes `<div class="ansi-line">`, empty output becomes `&nbsp;`, lines are concatenated | `ansi_lines_to_html`; ported `export-html-whitespace.test.ts` case and focused empty-line test |
| `Number.prototype.toString(16)` / `toString()` | Palette math uses JavaScript number formatting for gray/cube components and `rgb(...)` parameters | `js_number_to_radix16`, `js_number_to_string`; differential cases at 1e15, 2^53+1, 1.2e17, 1e100, and 1.8e308 exercise multi-digit hex and `Infinity` |

Differential: `oracle/coding-agent-ansi-to-html-oracle.ts` calls the real module (request `27004`, 47 cases) and compares both `ansiToHtml` and `ansiLinesToHtml`.

## Deviations

- Rust `&str` cannot carry lone surrogates; every other input, including control characters, NUL-separated lines, and multi-megabyte digit strings, is compared byte-for-byte.
