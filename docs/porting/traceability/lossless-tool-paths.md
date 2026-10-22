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
| code-index outline/symbol/references | native cwd, argv, custom stdout/stderr and render suffixes | custom callback argv/cwd/result unit test PASS; actual CLI execution differential pending |
| `GrepTool`, `GrepOperations`, context blocks | native cwd/path/glob, parsed event filename, custom read content and output prefixes | frozen TS missing-path callback/error differential PASS; raw context-prefix behavior test added; actual rg stream process proof pending |
| `FindTool`, `FindOperations`, result relativization | native cwd, paths, pattern, ignore list and result entries | frozen custom-glob callback order/arguments/raw result differential PASS; actual fd execution excluded |
| `EditTool`, `EditOperations`, diff preview cwd | raw paths and custom write content; native preview keys/error text | inside-pair edit callback/content/result/details compared to TS; preview UI integration pending |
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

- `edit-green-attempt.log`: **1/1 runner test PASS, zero warnings** using actual edit/edit-diff modules. Custom read returns an emoji; oldText matches its low surrogate; custom write receives the unmatched high unit plus replacement, unchanged by premature encoding. Exact callback paths, success text and diff/patch details equal frozen TS. Probe stubs do not prove actual diff styling or preview components.

- `find-red.log`: old module fails compilation against native path/render helpers; no behavioral RED.
- `find-green.log`: **1/1 runner test PASS, zero warnings**, actual find module calls custom exists/glob with raw paths and pattern, relativizes raw absolute results and matches frozen TS. Harness ToolManager refuses tool resolution, so native fd execution is not claimed.

- `grep-red.log`: old module fails compilation against native path/render helpers; no behavioral RED.
- `grep-green-attempt.log`: **1/1 runner test PASS, zero warnings**, actual grep custom metadata callback and original raw missing-path diagnostic equal frozen TS. Harness resolves a known rg path but failure precedes spawn; process/stream behavior not claimed.

## Utility checkpoint (after parent refresh `8f3aebbe`)

`utils::ansi::strip_ansi` is the single native UTF-16 API; removed `strip_ansi_js` and its surrogate sentinel/offset projection. Shared JsRegex removes ANSI ranges directly. `utils::shell::sanitize_binary_output` filters UTF-16 units, preserving retained unmatched units and pairs. `ansi-red.log` is compiler RED for the old API. `ansi-green.log`: 1/1 native cleanup test PASS using actual ANSI, shell and child-process modules, zero warnings. Frozen cleanup comparison was added after that log and remains pending execution.

## Gated payload bootstrap checkpoint (runtime pending)

Main granted `detached_job_bootstrap.rs`. Gated command/argv/cwd/environment, output/identity paths, stored identity path and SpawnFailed error are native. Split-output equality happens before encoding. Only actual output/identity opens/writes/rename/directory sync and Command argv/env/cwd encode. Runtime-process errors retain native messages inside the IO error carrier; Display alone encodes.

Added `lossless_detached_paths::distinct_raw_output_paths_reject_before_filesystem_or_spawn`, comparing frozen TS rejection of distinct raw paths that collide after Node UTF-8. **Unexecuted**, pending the real library. No detached process/lifetime proof yet. Outside direct production callers are the owned detached Bash/Pyrun runners; bootstrap integration tests require native fixtures.

## Detached artifact/controller checkpoint (runtime pending)

`DetachedJobArtifacts`, reserved/launched paths/job IDs, controller owner/session IDs, register cwd/worker/tool IDs, launch command/cwd/env, and manifest-writer callback paths/errors now retain native authorities. Creation/reservation joins raw paths and encodes only at mkdir; duplicate-artifact errors interpolate the originally requested raw directory even when two raw names collide at Node UTF-8. Controller filters and timestamp ordering use raw units. `DetachedJobError::message` retains raw control/launch/artifact errors; Display alone encodes.

`lossless_detached_paths.rs` compares real filesystem artifact creation and a surrogate-collision reservation error against frozen TS; **unexecuted**, pending the real coding library per main's instruction. No copied store/coordinator harness. Detached manifest/bootstrap/cleanup/control and dedicated caller tests remain migrating; no detached acceptance.

## Bash propagation checkpoint (compiler-boundary only)

Bash tool cwd, custom operations/spawn hooks, environment keys/values, command prefix, shell override and detached message/log-path/job-id fields are native. Local spawn/argv/environment and filesystem calls alone encode. Session IDs/files are native; approved scalar provider/model/reasoning metadata converts once into native env values. Call/result/preview/footer/warning rendering remains native through actual components. `finish_output` now returns close failures instead of dropping them; source close formatting uses the shared Node helper.

`lossless_bash.rs` records a custom hook/exec raw cwd/command/env test, **not executed yet**. `bash-detached-check.jsonl` probes the then-current in-progress source: 13 Bash primary diagnostics (12 scalar detached API dependencies, one constant prefix conversion fixed after the probe), not runtime proof. Detached chain remains pending, so no Bash acceptance. Outside callers: bwrap/backend.rs BashOperations implementation, AgentSession/SDK BashSessionEnvironment construction, bash option/operation tests. Main owns external production adapters; this lane owns dedicated tests.

## Output accumulator checkpoint

OutputSnapshot.content/full_output_path, retained tail text, temp-file prefix and configured/persisted paths are native. Removed the checked `truncation.content.to_utf8().expect(...)` projection. Decoded process bytes enter the native tail; byte counts and rolling tail clipping retain the frozen explicit Buffer boundary. Temp paths encode only at File::create. `accumulator-red.log`: compiler RED. `accumulator-green.log`: **1/1 actual-module test PASS, zero warnings**, original raw configured path remains in the snapshot while actual disk bytes/path match Node encoding.

Fixed the discovered Rust error suppression under main's [classified timing rule](output-accumulator.md#deviations): create/write errors use the uncaught crash path; flush/close errors return Err. `accumulator-{create,write}-behavior-red.log` each reproduces a false success (exit 0 instead of 1). `accumulator-error-green.log`: **3/3 tests PASS, one ignored subprocess entry actually invoked by the driver tests, zero warnings**, including real EACCES create and /dev/full ENOSPC write crashes. `accumulator-close-red.log` is the missing close seam compiler RED; `accumulator-close-green.log`: **1/1 PASS, zero warnings** for actual-file close-syscall EIO fault injection. No latched/fake-success errors remain. Outside callers: own Bash pipeline; `core/bash_executor.rs`, `modes/interactive/components/bash_execution.rs` and accumulator-based tests need native snapshot/path fixture adaptation.

## Supervisor path-utils caller

Removed `path.to_utf8_lossy()` before native read/general path resolution in `supervisor/main.rs`. Main subsequently granted the containment chain. `block_supervisor_file_access` accepts native roots; `resolve_existing_path` keeps resolved missing suffixes native, encoding only exists/canonicalize calls, and decodes the existing prefix returned by realpath. Containment compares native units. The helper now uses frozen `path.resolve`, not the old Rust-only home-expanding generic resolver. A unit test asserts an OS-replaced existing prefix plus distinct raw missing suffix; `supervisor-path-red.log` is compiler-blocked in pi-ai (12 errors), not runtime RED/PASS. No runtime security-policy proof yet.

## CLI file-processor checkpoint

`process_file_arguments` takes native cwd and accepts a slice of values convertible to JsString (including decoded CLI `String` argv and raw API paths); each path is resolved natively once. Attachment text and FileProcessorError.message are JsString. Only metadata/read/MIME filesystem calls encode paths. Image/text wrappers interpolate the original raw resolved filename. `file-processor-red.log`: compiler RED. `file-processor-green.log`: **1/1 test PASS, zero warnings**, actual-file original raw filename markup and empty arguments match frozen TS. Actual image processing is excluded from this bounded probe (explicit panic stub).

Outside caller: `main.rs` receives native text and `error.message(): &JsString`; its existing parsed `Vec<String>` file args remain accepted by the generic input without another legacy API. CLI file-processor regression tests need native cwd fixture adaptation.

## Shell configuration checkpoint

Main granted the whole shell module. `ShellConfig.shell/args` and `ShellError` messages are native; `get_shell_config` accepts `Option<&JsString>`; `ShellError::message` exposes the raw error while Display alone encodes. Shell lookup tests existence at the actual filesystem boundary, retains raw custom paths in diagnostics, and uses native lexical Windows-shim matching. `get_shell_env` returns `BTreeMap<JsString,JsString>`; consumers must encode keys/values only at spawn. External process lookup stdout is decoded into the native authority on entry.

Recorded Rust-only discrepancy: previous `Some("")` tried an empty custom path; frozen TS treats it as no override. Native selection now follows frozen truthiness. Source shell-search order remains unchanged. `shell-path-red.log`: missing native API compiler RED. `shell-path-green.log`: **1/1 actual-module frozen TS missing-shell raw-error differential PASS, zero warnings**. Default shell/Windows search/env process-boundary regression execution remains pending.

Outside source callers: `core/bash_executor.rs`, `core/config_value.rs`, `lib.rs` reexports; own `core/tools/bash.rs` is migrated in the next chain. Existing `tests/shell.rs` and shell differentials require fixture/type adaptation.

## Code-index checkpoint

`CodeIndexOperations::run` now receives `&[JsString]` argv and `&JsString` cwd; `CodeIndexCommandResult` stdout/stderr are native. Only LocalCodeIndexOperations encodes argv/cwd and decodes stream bytes. All three tool constructors/factories and render targets/suffixes are native. `lossless_code_index.rs` tests custom raw cwd/path/show arguments and retained output. `code-index-red.log`: compiler RED. `code-index-green.log`: **1/1 actual-module test PASS, zero warnings**. Existing trait-based code-index tests need signature/fixture adaptation; no real-process proof from this custom test.

## Actual tool renderer differential

`native_tool_renderers_match_frozen_raw_path_and_error_lines` exercises **six actual call/result renderers** (write/read/edit/ls/find/grep), actual Theme/Chalk, highlighter, diff/key-hints and native pi-tui Text/Box/Container. Forced hyperlink support verifies both retained styled path units and Node UTF-8 percent-encoding at the URL boundary. Raw multiline error text survives actual component rendering. Every returned UTF-16 line equals frozen TS, including ANSI controls and padding. `renderers-green.log`: **1/1 fixture PASS, six renderer pairs, zero warnings**.

`renderers-red{,-diagnostic}.log` exposed a fixture-ordering error, not a production defect: TS stores its call component inside state; Rust `ToolExecutionComponent` carries it explicitly in `ToolRenderContext.call_component` (tool_execution.rs:278). The test now reproduces that actual caller sequence before rendering the result/header. Config discovery, settings persistence and extension execution wrappers are still probe support stubs; no whole interactive-mode or actual terminal-write proof is claimed.

## Native diff/key hints checkpoint

Granted by main after `3e4511d8`: `components/diff.rs` and `keybinding_hints.rs`. Diff/intraline outputs now retain JsString through inverse/fg styling; removed every early encoding. Corrected old surrogate tests that explicitly expected replacement. Key formatting/hints return native strings, preserve raw key fragments/descriptions, and capitalize only the source first UTF-16 unit (including an untouched surrogate).

`diff-render-red.log` is compiler RED against native Theme. `diff-render-green.log`: **2/2 actual diff tests PASS, zero warnings**, with actual Theme, Chalk, highlighter and pi-tui linked; no Theme/TUI/diff/key-hint stubs remain. Settings persistence/config discovery/extension contracts remain bounded probe support, not integration proof. `native-theme-green.log` additionally compares native ANSI+binary cleanup to frozen TS (1/1 PASS).

All outside diff/key-hint caller paths recorded in `/tmp/pi-rust-repin-lossless-tools/key-hint-outside-callers.txt` (27 source/test/reexport paths); main/modes owner must adapt scalar formatting/line types. Shared tool callers are owned here and migrated with their modules.

## Factory and shared-render propagation checkpoint

All tool-index factories accept `&JsString` cwd. Shared coordination text extraction, text-component updates and themed output now return/accept the raw authority rather than encoding it. Runtime integration remains pending migration of bash/code-index and coordination callers; no compiling-factory claim.

## Sandbox operation caller checkpoint

Granted by main with refresh `8f3aebbe`: `bundled_extensions/bwrap/index.rs` Operations implementations and `operation_tests.rs`. Read/Write/Edit/Ls/Find operation paths/cwd/content and returned entry lists now forward JsString through worker JSON, without encoding before the worker. Extension-to-worker process transport continues using the existing lossless JSON writer. MIME extension lookup uses native `node_extname` rather than projecting the filename. New path-payload JSON roundtrip and raw-filename extension tests are **compiler-blocked**, not PASS. Existing missing-bwrap tests adapted to the widened APIs. SandboxParams.cwd/backend invocation are outside this Operations-only grant and remain scalar pending their owner's migration.

Outside utility callers requiring adaptation by main: `src/core/bash_executor.rs` (both APIs); `src/modes/interactive/components/bash_execution.rs` (rename `strip_ansi_js` to `strip_ansi`). External tests: `tests/{shell,ansi,differential_ansi,differential_default_footer,llama_ui,example_subagent,default_footer_extension,ask_questions_tool,llama_index}.rs`. Utility tests owned by this lane will be adapted separately.

Outside callers: `bundled_extensions/bwrap/index.rs` must widen its `WriteOperations` implementation to `&JsString` paths/content and preserve raw JSON payloads; `LsOperations` now takes `&JsString` and returns `Vec<JsString>` from `readdir`. `GrepOperations` now takes `&JsString` paths and returns `JsString` read content; bwrap must forward raw operation JSON. `FindOperations::exists/glob` now takes native paths/cwd, glob ignore list is `&[JsString]`, and results are `Vec<JsString>`; adapt bwrap's find implementation and tests. `EditOperations` now takes `&JsString` paths and write content; bwrap's edit bridge must forward these raw authorities. `ReadOperations` now receives `&JsString` for all paths; bwrap implementations and trait-based tests need the same widening. `ToolRenderContext.cwd` and native TUI `hyperlink` must accept native strings. These changes do not establish Q18 acceptance.
