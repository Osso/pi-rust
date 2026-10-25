# Traceability: `packages/coding-agent/src/core/export-html/index.ts`

Rust: `crates/pi-coding-agent-tools/src/core/export_html/index.rs` (templates embedded
from `src/core/export_html/templates/`). Tests: `tests/export_html.rs`,
`tests/differential_export_html.rs`.

Historical export proof: commit `532f7d12cd19235081bd4d6e425bde185652d86f`. Current target is the [common source lock](../source-lock.json); frozen4f9 storage-edge evidence below does not renew this entire historical export matrix.

## Baseline triage (2026-10-25)

The package fixture now reads templates from `pi-coding-agent-tools`, their current
crate location, rather than the retired coding-agent path. RED: file export
failed copying `template.html` (ENOENT). GREEN: `export_html` (4 parent tests)
and `differential_export_html` (2 parent tests; exact HTML for all 3 themes and
missing-file message) pass against frozen `4f9e6f8a4`. Child probes remain
parent-invoked. This repairs fixture setup; it does not renew unrelated rows.

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

Rust: `crates/pi-coding-agent-tools/src/core/export_html/tool_renderer.rs`. Tests:
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

Frozen storage edge: `repin_session_storage::export_uses_read_only_stale_archive_and_legacy_source` covers archive/stale export and legacy source immutability; initial RED File not found on stale path.

Bounded merged evidence and unresolved consumer/classification limits: [session storage integration](session-storage-integration.md). No fresh gate or whole-module promotion.

## Explicit asset override audit (2026-10-25)

Frozen reads five assets before resolving theme/session HTML. Native now reads through the existing binary-layout config helper when PI_PACKAGE_DIR is non-empty and embeds only without an override. Fixture packages expose both native `export-html` and frozen-tsx `src/core/export-html` layouts with equal bytes. RED `/tmp/ext-api2-html-red.log`: customized template marker was ignored. Differential also checks native/frozen missing-template read text after substituting only the established layout prefix. Custom-renderer empty-string truthiness is in the same focused fixture; proof pending.

## Q18 native HTML export (2026-10-22)

ExportOptions.theme_name, CSS/theme text, generated HTML, tool descriptions and
parameter JSON use native JsString/JsValue. Color matching uses native JsRegex;
first replacement + JS replacement expansion uses the single native string
helper. Session payload serializes directly from the native authority before
base64; HTML encodes only at the actual filesystem write. Theme/session errors
return native messages; write errors bind the original raw output path to NodeFS.

Raw tool description/schema payload, raw output-path write failure, and the
existing raw session payload fixtures are written but unexecuted while coding
compilation remains blocked. replacement-export-1 has zero export_html primaries
(before the new write-error fixture/boundary correction). Canonical replacement
leaf proof is separate:9 tests/180 Node differential cases PASS; do not infer
HTML runtime PASS from that leaf result.
