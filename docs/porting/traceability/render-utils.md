# coding-agent render-utils traceability

Pinned source: `packages/coding-agent/src/core/tools/render-utils.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::render_utils`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `render-utils.ts:12-19` | `shortenPath` returns `""` for non-strings, replaces the `os.homedir()` prefix with `~`, otherwise returns the input | `shorten_path`; `render_utils.rs::shorten_path_replaces_the_home_prefix` and differential `shorten` cases. Non-string input is excluded by the typed Rust API. |
| `render-utils.ts:21-25` | `linkPath` returns styled text unchanged without hyperlink support, otherwise wraps it in an OSC 8 link to `pathToFileURL(resolvePath(rawPath, cwd)).href` | `link_path`; `capability_dependent_output_matches_the_source` covers both capability states. `pi_tui::path_to_file_url` supplies the Node file URL. |
| `render-utils.ts:27-31` | `str` maps strings to themselves, `null`/`undefined` to `""`, every other type to `null` | `str_value`; `str_value_maps_types_like_the_source` plus differential `str` cases (number, boolean, object, array). |
| `render-utils.ts:33-35` | `replaceTabs` replaces each tab with three spaces | `replace_tabs`; focused and differential cases. |
| `render-utils.ts:37-39` | `normalizeDisplayText` removes every `\r` | `normalize_display_text`; focused and differential cases. |
| `render-utils.ts:41-66` | `getTextOutput` returns `""` for an undefined result, joins text blocks after `sanitizeBinaryOutput(stripAnsi(text))` + CR removal, and appends image fallbacks when images are unsupported or hidden | `get_text_output`; focused join/ANSI/control-character, undefined, and capability cases; differential text-block cases. Image fallback text comes from `pi_tui::image_fallback`, covered by the capability test. |
| `render-utils.ts:68-72` | `ToolRenderResultLike` generic alias | Not representable; Rust tools return `AgentToolResult`. |
| `render-utils.ts:74-76` | `invalidArgText` | `invalid_arg_text`; renderer differential `invalidArg`. |
| `render-utils.ts:78-85` | `renderToolPath` | `render_tool_path`; renderer differential `toolPath` cases (null, empty fallback, shortened, linked). |

Key-hint helpers used by the renderers (`formatKeyText`, `keyText`, `keyDisplayText`, `keyHint`, `rawKeyHint`) live in `render_utils` and read the coding-agent keybinding registry plus `current_theme()`, matching the source's module-global `theme`.

Differential: `oracle/coding-agent-render-utils-oracle.ts` calls the real pinned module (request `27100`, 23 cases) and compares results byte-for-byte.

## Discrepancies

- None observed. `getTextOutput` treats a `null` result as `""` in TypeScript only through the falsy check; Rust's `Option<&AgentToolResult>` makes the same distinction explicitly.
