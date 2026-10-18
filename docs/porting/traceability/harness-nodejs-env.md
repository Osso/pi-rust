# Node execution environment traceability

Pinned source: `packages/agent/src/harness/env/nodejs.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/env/nodejs.rs` (`NodeExecutionEnv`). Behavioral proof: `crates/pi-agent-core/tests/harness_env.rs` (23 tests). Differential proof: the 10 read/write/edit/bash fixtures in `crates/pi-agent-core/tests/differential_harness_tools/` execute through the real TS `NodeExecutionEnv`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `nodejs.ts:26-36` | Timeout validation: finite, positive, ≤ `2147483647/1000` seconds; timeout-coded errors | `resolve_timeout_ms`; `returns_non_zero_exit_codes_and_timeout_errors` |
| `nodejs.ts:50-71` | `~`, `~/`, `file://`, relative and absolute path resolution with lexical normalization | `expand_path`/`file_url_to_path`/`percent_decode`/`node_normalize`/`resolve_path`; `expands_home_relative_paths_and_file_urls` |
| `nodejs.ts:73-101` | `FileKind` detection and `FileInfo` from `lstat` without following symlinks | `file_info_from_metadata`; `reports_file_kinds_without_following_symlinks` |
| `nodejs.ts:103-124` | Node error code → `FileErrorCode` mapping plus `code: description, syscall 'path'` messages | `file_error`/`rename_error`/`error_parts`; exact ENOENT/ENOTDIR message tests |
| `nodejs.ts:126-128` | Pre-aborted operations return `aborted` with the resolved path | `abort_result`; pre-aborted operation test |
| `nodejs.ts:130-136` | `pathExists` via `access(F_OK)` | `path_exists` (metadata follows symlinks); used by shell discovery |
| `nodejs.ts:138-176` | `which bash` discovery with 5s timeout and first-line validation | `find_bash_on_path`/`run_command`; exercised by every bash test |
| `nodejs.ts:178-232` | Custom shell path, `/bin/bash`, PATH fallback, then `sh -c`; unavailable error message | `get_shell_config`; `returns_shell_unavailable_and_spawn_errors` |
| `nodejs.ts:234-247` | Environment merge: process env → `shellEnv` → call env; `inheritEnv: false` keeps only call env | `build_env`; shell-environment and replace-not-inherit tests |
| `nodejs.ts:249-269` | Kill the process group with SIGKILL on timeout/abort/cleanup | `kill_process_tree` (rustix `kill_process_group`), `process_group(0)` spawn; timeout, abort, cleanup tests |
| `nodejs.ts:271-337` | Settle after exit and stdio end, or 100 ms after exit with no data | `exec_command` exit wait plus fixed 100 ms reader grace before aborting readers; deterministic exec tests and differential bash fixtures |
| `nodejs.ts:352-356` | Constructor options cwd/shellPath/shellEnv | `NodeExecutionEnvOptions`/`NodeExecutionEnv::new` |
| `nodejs.ts:358-366` | `absolutePath`/`joinPath` | `absolute_path`/`join_path`; env tests |
| `nodejs.ts:368-501` | `exec`: pre-abort, timeout, cwd check, spawn, streaming callbacks, callback-error kill, timeout/abort/spawn result mapping, `exitCode ?? 0` | `exec_command`; exec, callback, timeout, abort, spawn-error tests and 10 bash differential fixtures |
| `nodejs.ts:503-514` | `readTextFile` UTF-8 | `read_text_file`; reads/writes test |
| `nodejs.ts:516-548` | `readTextLines` with `maxLines` stop and abort checks | `read_text_lines`/`split_lines` (LF, CRLF, CR); limit test |
| `nodejs.ts:550-562` | `readBinaryFile` | `read_binary_file`; reads/writes test |
| `nodejs.ts:564-582` | `writeFile` creates parent dirs and rechecks abort | `write_file`/`create_parent_dir`; write test |
| `nodejs.ts:584-594` | `appendFile` creates parent dirs | `append_file`; append test |
| `nodejs.ts:596-615` | `renameFile` abort check and source-path error | `rename_file`/`rename_error`; rename tests |
| `nodejs.ts:617-626` | `fileInfo` | `file_info`; metadata tests |
| `nodejs.ts:628-655` | `listDir` with per-entry `lstat` | `list_dir`; symlink listing test |
| `nodejs.ts:657-666` | `canonicalPath` via `realpath` | `canonical_path`; symlink test |
| `nodejs.ts:668-674` | `exists` is false only for `not_found` | `exists`; missing-path test |
| `nodejs.ts:676-683` | `createDir` recursive default | `create_dir`; create/remove options test |
| `nodejs.ts:685-693` | `remove` recursive/force defaults and directory error | `remove_path`; create/remove options test |
| `nodejs.ts:695-707` | `createTempDir` with prefix | `create_temp_dir`; temp test |
| `nodejs.ts:709-722` | `createTempFile` under `tmp-` with prefix/suffix | `create_temp_file`; temp test |
| `nodejs.ts:724-727` | `cleanup` kills active children and never fails | `NodeExecutionEnv::cleanup`/trait cleanup; cleanup tests |

## Exclusions and discrepancies

- Windows-only branches are excluded: `win32` Git Bash discovery, `where bash.exe`, `taskkill`, `~\\`, and legacy WSL `bash.exe -s` stdin transport. The port targets the Unix runtime.
- `createTempFile` uses a process-unique hex name instead of `randomUUID`; prefix/suffix and uniqueness contract are preserved.
- Node's `StringDecoder` is replaced by an incremental UTF-8 decoder; valid input is identical and invalid sequences become U+FFFD.
- The 100 ms post-exit stdio grace is a fixed wait rather than Node's data-resetting idle timer.
- `readTextFile` decodes with `String::from_utf8_lossy`, matching `TextDecoder` replacement behavior for invalid bytes.
- Node's error `path` property is preserved for every mapped error; unknown backend codes map to `unknown` with the source-shaped message prefix.
