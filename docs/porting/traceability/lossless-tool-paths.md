# Coding tool UTF-16 paths (Q18)

Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

## Write and shared rendering checkpoint

| Surface | Native authority | Proof / gap |
| --- | --- | --- |
| `WriteTool::{new,with_options}`, `create_write_tool{,_definition}` | `JsString` cwd | custom-operation frozen differential |
| `WriteOperations::{mkdir,write_file}` | `&JsString` paths/content | original cwd, directory, filename and content units compared to TS callbacks |
| `LocalWriteOperations` | encode at mkdir/write only | actual Node ENOENT message matches encoded error path; local success regression pending inclusion |
| `file_mutation_queue` | `JsString` map keys, raw missing-path key; canonicalized OS path on existing files | differential write traverses missing-path registration; collision/concurrency proof pending |
| `str_value`, `replace_tabs`, `normalize_display_text` | `JsString` | distinct lone units plus valid pair retained |
| `shorten_path`, `render_tool_path`, `get_text_output`, `link_path` | `JsString` | propagation implemented; source terminal integration pending native TUI and utility refresh |
| file URL | percent-encoded Node UTF-8 boundary | differential URL fixture pending |
| `LsTool`, `LsOperations` | raw cwd, callback paths and directory entries; UTF-16 locale sort | callback order, raw output and missing-path error compared to frozen TS |
| `ReadTool`, `ReadOperations` | raw cwd and callback paths, raw first-line overflow notice path and render labels | frozen TS callback/notice/details differential PASS; image branch and actual renderer integration excluded |
| local ls / rtk | Node UTF-8 at filesystem and process argv boundary | custom-operation test disables rtk; native execution regression pending |

`crates/pi-coding-agent/tests/lossless_tool_paths.rs` and `oracle/lossless-tool-paths-oracle.ts` exercise the actual frozen write module with custom operations, not a reimplementation.

## Proof ledger

Artifacts: `/tmp/pi-rust-repin-lossless-tools/`.

- `tools-red.log`: full coding target compiler-blocked (1381 errors); tests never execute. This is **not behavioral RED**.
- `standalone-red.log`: native helper/trait migration blocked by legacy utility signatures and a numeric formatting argument; **compiler-only**.
- `write-green-attempt.log`: standalone actual `#[path]` write/render-utils/path-utils/queue/node-fs modules; **2/2 runner tests PASS**. Harness uses explicitly bounded extension, Theme, Text and terminal capability stubs plus a source-equivalent binary sanitizer; it temporarily aliases actual legacy `strip_ansi_js` while parent coordinates replacement. This proves custom-operation execution and three pure helpers, **not** actual Theme/TUI rendering or final utility integration. Two harness dead-code warnings, no suppressed warnings.

- `mutant-write-path.log`: restoring premature cwd/path encoding fails the frozen callback-unit assertion (behavioral mutant killed, original restored).
- `local-error-red.log`: **PASS**, not RED despite the filename. Node already replaces unmatched path units in the actual filesystem ENOENT diagnostic. No speculative shared error formatter change required.
- `ls-green-attempt.log`: oracle used the wrong operation name (`isDirectory` instead of `stat`); rejected as proof. Fixture corrected to frozen interface.
- `write-ls-green.log`: **4/4 runner tests PASS, zero warnings** using actual write/ls/render-utils/queue/truncate/path modules. Same explicit harness support limits apply. Shared display helpers now compared to frozen TS too.

- `read-red.log`: full coding target remains compiler-blocked; no runtime RED.
- `read-green.log`: **1/1 runner test PASS, zero warnings**, original read module receives raw cwd/path through custom access/read callbacks; first-line overflow text and details equal frozen TS. Harness does not exercise image paths (explicit panic stub).

Outside callers: `bundled_extensions/bwrap/index.rs` must widen its `WriteOperations` implementation to `&JsString` paths/content and preserve raw JSON payloads; `LsOperations` now takes `&JsString` and returns `Vec<JsString>` from `readdir`. `ReadOperations` now receives `&JsString` for all paths; bwrap implementations and trait-based tests need the same widening. `ToolRenderContext.cwd` and native TUI `hyperlink` must accept native strings. These changes do not establish Q18 acceptance.
