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

Per-file byte identity, inclusion constants, runtime output wiring and current named pass evidence live in [asset traceability](export-html-assets.md).

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

## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).
