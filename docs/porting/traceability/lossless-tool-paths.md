# Coding tool UTF-16 paths (Q18)

Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

## Write and shared rendering checkpoint

| Surface | Native authority | Proof / gap |
| --- | --- | --- |
| `WriteTool::{new,with_options}`, `create_write_tool{,_definition}` | `JsString` cwd | custom-operation frozen differential |
| `WriteOperations::{mkdir,write_file}` | `&JsString` paths/content | original cwd, directory, filename and content units compared to TS callbacks |
| `LocalWriteOperations` | encode at mkdir/write only | existing local-write regression pending standalone inclusion; error-message paths remain under investigation |
| `file_mutation_queue` | `JsString` map keys, raw missing-path key; canonicalized OS path on existing files | differential write traverses missing-path registration; collision/concurrency proof pending |
| `str_value`, `replace_tabs`, `normalize_display_text` | `JsString` | distinct lone units plus valid pair retained |
| `shorten_path`, `render_tool_path`, `get_text_output`, `link_path` | `JsString` | propagation implemented; source terminal integration pending native TUI and utility refresh |
| file URL | percent-encoded Node UTF-8 boundary | differential URL fixture pending |

`crates/pi-coding-agent/tests/lossless_tool_paths.rs` and `oracle/lossless-tool-paths-oracle.ts` exercise the actual frozen write module with custom operations, not a reimplementation.

## Proof ledger

Artifacts: `/tmp/pi-rust-repin-lossless-tools/`.

- `tools-red.log`: full coding target compiler-blocked (1381 errors); tests never execute. This is **not behavioral RED**.
- `standalone-red.log`: native helper/trait migration blocked by legacy utility signatures and a numeric formatting argument; **compiler-only**.
- `write-green-attempt.log`: standalone actual `#[path]` write/render-utils/path-utils/queue/node-fs modules; **2/2 runner tests PASS**. Harness uses explicitly bounded extension, Theme, Text and terminal capability stubs plus a source-equivalent binary sanitizer; it temporarily aliases actual legacy `strip_ansi_js` while parent coordinates replacement. This proves custom-operation execution and three pure helpers, **not** actual Theme/TUI rendering or final utility integration. Two harness dead-code warnings, no suppressed warnings.

Outside callers: `bundled_extensions/bwrap/index.rs` must widen its `WriteOperations` implementation to `&JsString` paths/content and preserve raw JSON payloads. Tests implementing this trait need the same changes. `ToolRenderContext.cwd` and native TUI `hyperlink` must accept native strings. These changes do not establish Q18 acceptance.
