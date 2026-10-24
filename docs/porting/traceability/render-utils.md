# coding-agent render-utils traceability

## Native shared Text support

`set_text_component` moves the existing strict ls reuse/downcast behavior unchanged into one helper for ls/find. Reuse preserves component identity and updates text; incompatible prior kinds return original native TypeError, never an alternate replacement. This is native support for repeated frozen renderer callback logic, not an additional TS export or whole render-utils re-pin. Find16/LS16 frames and raw-path controls GREEN14/0 (`find-render-green.log`); find's19-frame follow-up includes two frozen incompatible-kind TypeErrors with retained native source and successful subsequent reuse (`find-render-branding-green-2.log`).

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

Key-hint helpers (`formatKeyText`, `keyText`, `keyDisplayText`, `keyHint`, `rawKeyHint`) are imported from `modes::interactive::components::keybinding_hints`, the module matching the source import. `render_utils` has no copies.

Differential: `oracle/coding-agent-render-utils-oracle.ts` calls the real pinned module with the built-in dark theme (request `27100`, 23 cases) and compares real ANSI output byte-for-byte.

## Discrepancies

- `getTextOutput` treats a `null` result as `""` in TypeScript only through the falsy check; Rust's `Option<&AgentToolResult>` makes the same distinction explicitly.

## Q5 current-pin homedir boundary (2026-10-23)

Frozen 4f9e6f8a4 `shortenPath` invokes `os.homedir()`. Native shortening now invokes the [shared POSIX authority](node-os.md), preserving HOME unset passwd lookup and explicit SystemError failure. `shorten_path` returns `Result<JsString, NativeNodeError>`; path linking/rendering use the existing fallible PathError carrier, propagating its message to the renderer HandlerError boundary. Removed the empty-HOME alternate path. The read/write/edit/ls/find/grep/code-index formatters propagate the failure to their existing HandlerError renderer boundary. ToolExecutionComponent already catches renderer errors; no new catch/alternate display was introduced.

`differential_render_home::unset_home_and_failed_passwd_lookup_match_frozen_renderer_boundary` compares helper output, direct read-call rendering and actual ToolExecutionComponent frames in isolated native/Node processes with HOME removed. A test-only LD_PRELOAD getpwuid_r interposer forces EIO (errno and return value), after Node imports. Both passwd success and failure match frozen; RED render-home-red.log and GREEN render-home-green.log. The broader NSS-return/errno distinction remains the approved shared gap, not a renderer fallback.

Mechanical callers: read/write/ls/find/grep public renderCall and definition callback registration; edit call/result component filling; code-index outline/symbol/references call formatters and registrations. Existing interactive ToolExecutionComponent error handling needs no change; the new differential exercises it.

Q5 extension: the same process matrix now includes collapsed tilde paths, normalization/resolution and canonicalization's source catch. utils/paths::home_directory returns a Result rather than panic_any, so the explicit read-classification error reaches the renderer boundary. RED render-home-tilde-red.log captured the uncaught native panic; GREEN render-home-tilde-green.log covers absolute/tilde renderer paths. Shared path proof remains module-partial.

Hyperlink extension: `link_path` now propagates path-resolution failures instead of `expect` panicking. Its URL encoder consumes the already-absolute resolved path without a second fallible resolve. Read/write/edit/ls formatters mechanically propagate PathError to their existing renderer HandlerError boundary. An expanded read with hyperlinks and a deleted cwd reproduces the old panic at the exact dependency boundary (read-renderer-dependent-errors-red.log). Package-dir error propagation is the remaining case in that matrix until its separate config correction.
