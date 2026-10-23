# Shared Node filesystem errors

## Local Operations NUL drift — RED-intent only

`pi-coding-agent/tests/lossless_local_path_validation.rs` compares 13 real local read/range/stat/access/write/mkdir/edit/ls/grep Operations to Node's `ERR_INVALID_ARG_VALUE` with short/long paths containing NUL and unmatched UTF-16 units. It asserts exact inspected message, no OS errno, and unchanged directory contents. Test committed unexecuted while the coding library is blocked; production validation remains queued until in-crate RED. Existence probes intentionally retain Node's false-on-invalid-path contract; process spawning is a separate boundary.

Rust: `crates/pi-js-compat/src/node_fs.rs`. Pure compatibility prerequisite for Node builtin filesystem errors, not a new TypeScript application module. Node reference: `oracle/node-fs-error-oracle.ts`, executed through existing `TsOracle` source-lock verification, tsx and parent watchdog. All filesystem fixtures are isolated temporary paths; no providers or network.

| Public API / branch | Behavioral proof |
|---|---|
| `NodeFsError`, `node_fs_error_record`: Error name, message, negative Unix errno, code, syscall, optional path | `pi-js-compat/tests/node_fs_errors.rs::real_filesystem_messages_and_own_metadata_match_node`: 13 actual filesystem cases, exact message and complete own metadata (stack excluded from comparison) |
| `NodeFsError::console_message`: uncaught Error line and own-property block, optional spawnargs, inspect quoting/layout | `pi-evals/tests/run_evals.rs`: Node ENOTDIR/EACCES and spawnSync ENOENT differentials; `node_fs_errors.rs::console_messages_match_node_inspection`: Node inspect comparison for quotes, controls and array truncation |
| `node_fs_error`: message projection | Same corpus: missing-parent write/read, directory write/read, ENOTDIR write/read, EEXIST mkdir, symlink-loop read |
| `NodeReadApi`, `node_read_error_record`: select `ReadFileSync` or `PromisesReadFile`; read syscall for EISDIR, open otherwise | Same corpus compares directory, missing-parent and ENOTDIR async errors, plus pathless sync whole-file and fd reads, including complete own metadata |
| `node_read_error` / `node_promises_read_error`: API-specific message projections | Same corpus plus coding-agent `tests/node_fs_errors.rs` (3 tests at preserved public paths); `differential_edit_diff` case 27 exercises pinned promises.readFile |
| `node_error_code`, `node_errno_name`, `node_errno_description`: shared libuv table, exact unknown-number diagnostics; owned error codes | `pi-coding-agent/tests/differential_libuv_errors.rs`: entire runtime map (85 entries) and unknown -126/-9999; synthetic ErrorKind mapping remains explicit |
| ErrorKind-only construction | 8 kinds retain existing messages/code/path; errno stays absent, including directory-read legacy path |
| Raw OS descriptions | All 85 names/descriptions captured from Node 26.11.1 / libuv 1.52.1 on Linux x64; no OS diagnostic fallback; ETXTBSY is `text file is busy` |
| Pathless `uv_cwd` / deleted current directory | `node_fs_errors.rs::removed_current_directory_error_has_node_message_and_pathless_own_fields`; actual generator original-source/real-native-binary deleted-CWD differential preserves message, negative errno, code and syscall before fetching |

## Q18 native per-origin contract

Approved by parent against the common pin on 2026-10-21: path/program inputs and record syscall/path/message fields are `JsString`. Callers supply raw units, never pre-repaired paths. The single ordinary filesystem error-record builder models Node's already-crossed OS boundary by decoding its UTF-8 path back to native text (unpaired units become U+FFFD). `NodeReadApi` still distinguishes synchronous and promises EISDIR metadata.

| Origin | Required path behavior | Behavioral fixture |
|---|---|---|
| fs sync | Error path/message reflect UTF-8 boundary repair | `lossless_node_fs::fs_sync_repairs_only_the_crossed_filesystem_path` |
| fs.promises | Same repair; retain API-specific EISDIR path behavior | `fs_promises_repairs_only_the_crossed_filesystem_path` |
| child_process spawn | Executable remains raw in syscall/path/message | `spawn_keeps_raw_executable_in_message_syscall_and_path` |
| Unix connect/listen | Address text remains raw | `sockets_keep_raw_address_in_connect_and_listen_errors` |
| NUL path validation | Inspect original UTF-16 before any FS call; truncate inspection by units | `nul_validation_inspects_raw_units_before_any_boundary_repair` |

Evidence: direct builtin Node probes covered open/read/write/mkdir/stat/access/readdir in both sync/promises APIs, spawn/spawnSync, and Unix connect/listen. In-crate runner-backed RED `/tmp/pi-rust-repin-lossless-tools/node-origin-red.log` passed the two fs-origin cases and failed spawn/socket/NUL because scalar APIs forced premature encoding. No mock filesystem or probe crate is involved. In-crate GREEN at revision `22c4fc90`: `node-origin-green.log`, 5/5 passed. Existing shared scalar filesystem/inspection regression suite passed 7/7 at `089f9041`, `/tmp/pi-rust-repin-lossless-tools/node-existing-green.log`.

Additional observed origin distinction: rmSync's EISDIR directory validation retains raw path text; its OS-origin EACCES message repairs the path. The two branches remain explicit in `node_rm_error`, not a blanket sanitizer. Console inspection escapes lone units and preserves valid pairs.

Owned local Operations pass raw paths to the shared formatter and encode only at FS calls. `core/tools/node_fs::operation_error` uses the existing native `HandlerError` IO carrier; `operation_error_message` recovers its sole message authority. `lossless_tool_paths::native_operation_error_survives_the_io_carrier` covers it. Tool wrapper spill paths/footer/details are native (`lossless_tool_output_spill`); coding-library execution remains blocked. Outside callers are assigned to main/owners in the lane inventory.

## Q18 close-error prerequisite

Main granted pathless close and raw OS EIO formatting for spill-close propagation. `node_fs_errors::pathless_close_and_eio_messages_match_nodes_uv_exception` invokes the actual Node `internal/errors` UVException constructor for `close` without a path and `open` with a path. `node-close-red.log` proves the prior erroneous empty quoted path and std EIO description. Native OS EIO now uses `i/o error`; a constructed ErrorKind-only custom diagnostic is not overwritten. Close with an empty path omits both metadata and quoted suffix. GREEN and existing constructed-kind regression execution pending at this checkpoint; no broad strerror claim.

## Classified conformance fixes

Earlier Node RED proved pathless **synchronous** EISDIR/read and the ELOOP description. That fix overgeneralized EISDIR/read to promises.readFile, causing the edit-diff regression introduced by e897a4d. Runner-backed RED now proves promises.readFile must retain both path metadata and message suffix. `NodeReadApi` distinguishes the originating whole-file API in the shared formatter; raw syscall/synchronous callers remain pathless. EISDIR/open retains path. No pinned TS source bug changed.

`core::tools::node_fs::{node_fs_error,node_read_error}` and `utils::child_process::{node_errno_name,node_error_code}` re-export the shared authority; `node_error_code` now returns owned strings for unknown-number diagnostics. The model-data private adapter wraps the shared message in `JsString`; reads select `read` for directory errors. There is no coding-agent dependency from pi-ai. Existing model-data ELOOP and directory-read behavior is retained.

Deleted-CWD generator proof first failed with native generic OS text (`80-deleted-cwd-red.log`); the shared record now preserves Node's contextual ENOENT `process.cwd` message and omits path for `uv_cwd`. Native/source CLI GREEN81 and shared filesystem suite GREEN82 are recorded under `/tmp/pi-rust-model-generator-proof`. Only filesystem foundation proof invalidates; number/date/JSON scopes are unchanged.

## Filesystem caller audit (runtime pin 532f7d12)

Paths below are relative to `crates/`; TS paths are relative to `packages/`.
Only edit-diff selected the pathless raw-read formatter for a proven promises
whole-file EISDIR/read. Other generic `open` adapters are unchanged; this audit
is not additional proof of every caller's syscall-selection branch.

| Rust caller | Originating pinned Node API / disposition |
|---|---|
| `pi-coding-agent/src/core/tools/edit_diff.rs::read_editable_file` | `coding-agent/src/core/tools/edit-diff.ts`: promises `access`, `readFile`; read failures now use `node_promises_read_error` |
| `pi-coding-agent/src/core/tools/read.rs` | `coding-agent/src/core/tools/read.ts`: promises `access`, `readFile`, `stat`, `open` and `FileHandle.read`; range `read` is a raw fd failure, not promises.readFile; generic access/open/stat adapters unchanged |
| `pi-coding-agent/src/core/tools/edit.rs` | `coding-agent/src/core/tools/edit.ts`: promises `readFile`, `writeFile`; existing generic open adapters unchanged (no raw read/path omission) |
| `pi-coding-agent/src/core/tools/write.rs` | `coding-agent/src/core/tools/write.ts`: promises recursive `mkdir`, `writeFile`; mkdir/open adapters unchanged |
| `pi-coding-agent/src/core/tools/ls.rs` | `coding-agent/src/core/tools/ls.ts`: promises `stat`; stat adapter unchanged |
| `pi-coding-agent/src/core/tools/tool_definition_wrapper.rs` | `coding-agent/src/core/tools/tool-definition-wrapper.ts`: `writeFileSync`; open adapter unchanged |
| `pi-coding-agent/src/config.rs` | `coding-agent/src/config.ts`: `readFileSync`; `node_read_error` remains synchronous |
| `pi-coding-agent/src/modes/interactive/theme/mod.rs` | `coding-agent/src/modes/interactive/theme/theme.ts`: `readFileSync`, `readdirSync`; sync read and scandir adapters unchanged |
| `pi-coding-agent/src/modes/interactive/external_editor.rs` | `coding-agent/src/modes/interactive/external-editor.ts`: `mkdtempSync`, `writeFileSync`, `readFileSync`; sync adapters unchanged; chmod implements mkdtempSync's 0700 mode (no separate TS chmod call) |
| `pi-coding-agent/src/bundled_extensions/run_plan.rs` | `coding-agent/extensions/run-plan/src/index.ts`: promises `readFile`, `readdirSync`; existing open/scandir adapters unchanged (no raw read/path omission) |
| `pi-coding-agent/src/bundled_extensions/docs_tree_context.rs` | `coding-agent/extensions/docs-tree-context/src/index.ts`: promises `stat`, `readdir`; stat/scandir adapters unchanged |
| `pi-ai/src/model_data_tools.rs` | `ai/scripts/model-data.ts`: `readFileSync`, `readdirSync`; sync read/scandir adapters unchanged |
| `pi-ai/src/image_catalog_generator.rs` | `ai/scripts/generate-image-models.ts`: `writeFileSync`; open/write adapters unchanged |
| `pi-ai/src/model_catalog_generator/artifacts.rs` | `ai/scripts/generate-models.ts`: `readFileSync`, `writeFileSync`, `mkdirSync`, `rmSync`, `renameSync`, `mkdtempSync`; raw read/open/mkdir/lstat/rm/unlink/rename adapters unchanged |
| `pi-ai/src/model_catalog_generator.rs` | `ai/scripts/generate-models.ts`: `process.cwd`; pathless uv_cwd unchanged |
| `pi-evals/src/run_evals.rs` | `evals/scripts/run-evals.mjs`: `mkdirSync`, `spawnSync`; mkdir/spawnSync records unchanged |
| `pi-coding-agent/src/core/tools/node_fs.rs` | Public re-exports only; adds async projection without changing old signatures |

E2 proof: `/tmp/pi-rust-runner-guard-fixes/ledger.md`; shared metadata RED and
edit-diff case-27 RED precede implementation. Sync callers retain their old API.

## Queued source discrepancy

Local read/edit/write/ls Operations still enter Rust filesystem calls without the source's pre-IO `node_path_argument_error` validation. A NUL path can therefore become an OS-style EINVAL or generic IO diagnostic rather than Node's raw-inspected TypeError. This is pre-existing caller drift, not supported parity. Parent requires a separate frozen-behavior RED-first correction after transport identity/probe and cleanup corrections, once the coding library compiles. The shared native validator itself is already covered by the Node API-class differential.

## Explicit gaps

- Exact negative errno and filesystem differential proof are Linux/Unix bounded; Windows libuv errno translation is not asserted.
- Unknown errno descriptions retain the existing std text rather than claiming complete Node conformance.
- Existing `node_errno_name(0)` panics in rustix; zero is not a filesystem failure errno. Preserved outside this observed-error slice, not tested as supported behavior.
- The record contains no stack. `console_message` renders the error line and own properties without fabricating JS frames; generator-specific stack presentation belongs to its caller.
- Typed Rust inputs exclude invalid JS argument types. Paths are native UTF-16 strings; arbitrary non-UTF-8 OS path parity is not established.

## Focused proof

Ledger: `/tmp/pi-rust-shared-node-fs-proof`; RED artifacts `-red2.log` (1 pass/1 fail), `-red3.log` (1 pass/2 fail). Shared metadata corpus passed on first implementation; an invalid zero-errno test exposed the inherited gap above and was removed without changing production. Final focused command results and revision are recorded in ledger. No check, clippy, readability, coverage, broad suite or final gates run; integration belongs to parent.

## Q5 file-handle read origin (2026-10-23)

`node_file_read_error_record` represents read errors from an already-open descriptor: no path property, syscall read, unchanged Error constructor/name, errno and code. The formatter now accepts explicit absent path internally; existing path-origin APIs still supply Some(path). Direct Node pipe pread proved ESPIPE uses `invalid seek`, not Rust's decorated IO error text. `differential_mime_file_errors` asserts the complete record against that actual Node error and real frozen MIME directory-read failure. Existing whole-file read API distinctions remain unchanged.
