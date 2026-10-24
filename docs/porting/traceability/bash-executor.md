# coding-agent bash-executor traceability

## Current-pin spill/stream cycle (open)

Frozen `4f9e6f8a4`. `differential_bash_executor_spill` runs the actual source module with seventeen concrete histories and isolated native processes. Valid RED8 (`bash-executor-spill-red-5.log`) proves collector-lock reentry deadlock, ignored mutable TMPDIR/TMP/TEMP, non-native open panic before callback delivery, silent writes/close failures and missing ordinary-error close. Added late-write histories produce separate RED2 (`bash-executor-spill-red-6.log`) for source `ERR_STREAM_WRITE_AFTER_END` after normal/error completion; the original reentry/path fixes already pass in that run. Complete output/callback/file hashes, byte/line-only thresholds, UTF-16 rolling counts, sanitized bytes, permissions and random basename shape are compared, not source text or internal calls.

Native state now closes on every declared return path, releases locks before user callbacks, preserves closed/failed stream behavior and dispatches original Node IO errors through shared uncaught authority after callback delivery. No writer queue, fallback or production injection seam added. Native file IO remains synchronous; error dispatch is a separate native thread, so custom operations do not acquire a Tokio-runtime prerequisite merely to report stream errors. Exact independent-producer scheduling remains under the approved Node-runtime-oddity policy, not new parity evidence. [Shared POSIX tmpdir contract](../../specs/node-tmpdir.md) replaces the executor/accumulator's duplicate std getter. Initial close fixtures could not affect libuv's direct close syscall and are invalid error evidence; the final fixture invalidates the actual spill descriptor before asynchronous uv_fs_close, producing real EBADF. GREEN17 at5aeea396 and after the fixture readability refinement31b2fbfd (`bash-spill-regressions.log`), with retained executor/core/stream/lossless cases. Workspace0/0 and owned clippy0 at31b2fbfd; coding-agent global1089 remain explicit (`bash-spill-workspace.*`, `bash-spill-clippy-agent.*`). This is bounded spill evidence, not whole bash closure.

## Current-pin numeric prerequisite

Frozen `4f9e6f8a4` `bash-executor.ts` declares `BashResult.exitCode: number | undefined` and forwards `result.exitCode` from custom operations. The [bash re-pin](bash.md) widens this carrier/collector parameter to `Option<f64>` without changing collector logic; integer serialization uses the shared number adapter. Parent cleared mechanical session/UI consumers. `differential_bash_core::bash_executor_preserves_custom_binary64_exit_values` compares10 direct executor returns against frozen (null/zero/fractions/beyond-i32/exponent/NaN/±Infinity/negative-zero), preserving values before JSON projection; GREEN with a direct negative-zero sign assertion (`bash-exit-executor-2.log`). This is not whole-module current-pin proof; existing streaming/spill/error discrepancies remain under their original scope.

Pinned source: `packages/coding-agent/src/core/bash-executor.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/core/bash_executor.rs`. Tests: `crates/pi-coding-agent/tests/bash_executor.rs` (6 behavioral).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `BashExecutorOptions` (`onChunk`, `signal`) | `BashExecutorOptions`, `BashChunkCallback` | streaming test |
| `BashResult` | `BashResult` | all tests |
| `executeBashWithOperations` | `execute_bash_with_operations` | all tests |
| streaming sanitization (`stripAnsi` + `sanitizeBinaryOutput` + `\r` removal) | `OutputCollector::push` | `preserves_sanitization_when_using_local_bash_operations` |
| `TextDecoder(..., { stream: true })` | Shared `pi-js-compat::text_decoder::Utf8StreamDecoder`; old duplicate removed | Current ten-case `differential_bash_executor_stream` includes initial/split BOM and incomplete tail |
| rolling buffer (`DEFAULT_MAX_BYTES * 2`, drop oldest chunks) | `CollectorState::push` | truncation test |
| `ensureTempFile` (`pi-bash-<16 hex>.log`, replay existing chunks) | `CollectorState::ensure_temp_file`, `random_hex_8` | `persists_full_output_when_truncation_happens_by_line_count_only` |
| `truncateTail` result and full-output path | `truncate_tail`, `BashResult` | truncation test asserts 3000 lines |
| cancelled branch (`exitCode: undefined`) | `BashResult.exit_code = None` | `reports_cancellation_without_an_exit_code` |
| error path rethrow unless aborted | `execute_bash_with_operations` error branch | cancellation test |
| streaming callback | `BashExecutorOptions.on_chunk` | `streams_sanitized_chunks_to_the_callback` |

## Q18 native checkpoint

Common pin: `4f9e6f8a4`. `execute_bash_with_operations` accepts native command/cwd. `BashResult.full_output_path` and the collector's sole temp-path authority are `JsString`; temp-root text is decoded from the OS once and file creation encodes only at the filesystem call. Output and callback strings remain native.

`tests/lossless_bash_executor.rs` exercises raw command/cwd forwarding to custom Operations and verifies a native full-output path contains complete spilled bytes. The six existing behavioral tests use native contracts. All runtime execution remains pending coding-library compilation per parent instruction; no additional standalone probe was created. This checkpoint does not establish spill-I/O error parity.

Gaps / deviations:

- Superseded by current spill cycle: file IO is native synchronous; callbacks are outside locks and uncaught failures are separate asynchronous events. Exact scheduling between independent IO/operation producers is not reproduced under the approved runtime policy. Values and error boundaries require current proofs, not an identical-order claim.
- Temp files are created with `File::create` (mode from umask); Node uses default stream flags.
- The source never flushes the decoder at the end; the port also drops an incomplete trailing sequence.
