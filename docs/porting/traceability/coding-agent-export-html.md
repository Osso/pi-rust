# Traceability: `packages/coding-agent/src/core/export-html/index.ts`

Rust: `crates/pi-coding-agent/src/core/export_html/index.rs` (templates embedded
from `src/core/export_html/templates/`). Tests: `tests/export_html.rs`,
`tests/differential_export_html.rs`.

Pinned source: commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

## Exported surface

| TypeScript | Rust | Tests |
|---|---|---|
| `ToolHtmlRenderer` | `ToolHtmlRenderer` trait | `tests/tool_renderer.rs`, live-session export |
| `ExportOptions` | `ExportOptions` (+ `from_output_path` for the `string` overload) | both export tests |
| `exportSessionToHtml` | `export_session_to_html` | `exports_live_sessions_with_prompt_and_tools`, `rejects_in_memory_sessions` |
| `exportFromFile` | `export_from_file` | `exports_file_sessions_with_embedded_session_data`, differential (3 themes + missing file) |
| `RenderedToolHtml` | `RenderedToolHtml` (serde camelCase, absent fields omitted) | `pre_render_custom_tools` structure |
| `SessionData` (internal) | `SessionData` | decoded from the exported `<script id="session-data">` |
| `ToolSummary` (inline) | `ToolSummary` | live-session export test |

## Branches

| TypeScript | Rust | Tests |
|---|---|---|
| `parseColor` (hex, `rgb()`) | `parse_color` | theme derivation through dark/light differential exports |
| `getLuminance` | `get_luminance` | same |
| `adjustBrightness` | `adjust_brightness` | same |
| `deriveExportColors` (light/dark/unparseable) | `derive_export_colors` | dark/light exports |
| `generateThemeVars` | `generate_theme_vars` | HTML CSS assertions + differential |
| `generateHtml` template/CSS/JS/session-data replacement | `generate_html` | byte-for-byte differential |
| JavaScript `String.replace` `$`-substitution | `pi_js_compat::string::replace` | differential (highlight.js contains `$&` and `$$`) |
| base64 session payload | `pi_js_compat::base64::btoa` | decoded session-data assertions + differential |
| `TEMPLATE_RENDERED_TOOLS` | `TEMPLATE_RENDERED_TOOLS` | live-session export (custom calls absent) |
| `preRenderCustomTools` | `pre_render_custom_tools` | live-session export; `tool_renderer.rs` |
| in-memory / empty / missing-file errors | `ExportError` messages | `rejects_in_memory_sessions`, differential missing file |
| default output name `APP_NAME-session-<basename>.html` | `default_output_path` | not directly tested (would write into the process cwd) |

## Embedded assets

Byte-identical copies of the pinned source assets live in
`crates/pi-coding-agent/src/core/export_html/templates/` and are compiled in with
`include_str!`:

| Asset | sha256 |
|---|---|
| `template.html` | `916782b1184a9597527605ad751e2b3af30fcea23ba2194002969cd217a06881` |
| `template.css` | `28c16e3827c23a62eef8283cac316b478f946e023093adad25ff9c9b891d41af` |
| `template.js` | `1893cdb77587f592eef5717905391269886d6d7a4dc6a488417a73da374d9226` |
| `vendor/marked.min.js` | `d5487edc7258b404bfa74c393d74a6393155f02517bd5e7e77cd64f8187f39a0` |
| `vendor/highlight.min.js` | `837a6fa5b0c736b52bbde2b2b6190f305da3fc9ed41681db5321507057b5c846` |

Source paths: `packages/coding-agent/src/core/export-html/{template.html,template.css,template.js,vendor/*}`.

## `tool-renderer.ts`

Rust: `crates/pi-coding-agent/src/core/export_html/tool_renderer.rs`. Tests:
`tests/tool_renderer.rs` (9 tests, including the ported whitespace test).

| TypeScript | Rust | Tests |
|---|---|---|
| `ToolHtmlRendererDeps` | `ToolHtmlRendererDeps` | every test |
| `createToolHtmlRenderer` | `create_tool_html_renderer` | every test |
| `isBlankRenderedLine` / `trimRenderedResultLines` | same names | `trims_tui_spacing_lines_from_custom_tool_result_html` |
| render-call cache + last component | `rendered_call_components` | `passes_args_and_previous_component_identity` |
| render-result collapsed/expanded cache | `rendered_result_components` | `omits_...`, `includes_...` |
| `renderedStates` / `renderedArgs` | `rendered_states` / `rendered_args` | `shares_render_state_between_calls` |
| missing renderer → `undefined` | `Option::None` | `returns_none_without_a_custom_renderer` |
| `catch` → `undefined` | Rust renderers cannot reject; no path | — |
| default width 100 | `width.unwrap_or(100)` | `renders_call_html_with_the_default_width` |

The renderer is `Rc`-based (like `ComponentRef`) and therefore not `Send`; the
TypeScript object is likewise single-threaded. `ToolRenderContext.args` is
`Value::Null` before the first `renderCall`, while the source value is
`undefined`.

## Gaps / deviations
- The default output filename is implemented but not tested to avoid writing into
  the test process cwd.
- Session-entry key order comes from the session-manager serializer. Pi-written
  sessions use the canonical append order that the differential fixture follows;
  arbitrary hand-edited key orders are reordered by `SessionEntry` serialization
  (session-manager limitation, not this module).
- `SessionHeader` now preserves explicit `null` values for keys present in the
  source file, so `parentSession: null` survives the export payload.
