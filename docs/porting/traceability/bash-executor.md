# coding-agent bash-executor traceability

Pinned source: `packages/coding-agent/src/core/bash-executor.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/core/bash_executor.rs`. Tests: `crates/pi-coding-agent/tests/bash_executor.rs` (6 behavioral).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `BashExecutorOptions` (`onChunk`, `signal`) | `BashExecutorOptions`, `BashChunkCallback` | streaming test |
| `BashResult` | `BashResult` | all tests |
| `executeBashWithOperations` | `execute_bash_with_operations` | all tests |
| streaming sanitization (`stripAnsi` + `sanitizeBinaryOutput` + `\r` removal) | `OutputCollector::push` | `preserves_sanitization_when_using_local_bash_operations` |
| `TextDecoder(..., { stream: true })` | `StreamingUtf8Decoder` (invalid sequences → U+FFFD, incomplete tail buffered) | exercised by shell output decoding |
| rolling buffer (`DEFAULT_MAX_BYTES * 2`, drop oldest chunks) | `CollectorState::push` | truncation test |
| `ensureTempFile` (`pi-bash-<16 hex>.log`, replay existing chunks) | `CollectorState::ensure_temp_file`, `random_hex_8` | `persists_full_output_when_truncation_happens_by_line_count_only` |
| `truncateTail` result and full-output path | `truncate_tail`, `BashResult` | truncation test asserts 3000 lines |
| cancelled branch (`exitCode: undefined`) | `BashResult.exit_code = None` | `reports_cancellation_without_an_exit_code` |
| error path rethrow unless aborted | `execute_bash_with_operations` error branch | cancellation test |
| streaming callback | `BashExecutorOptions.on_chunk` | `streams_sanitized_chunks_to_the_callback` |

Gaps / deviations:

- The collector is synchronous behind a mutex; Node's `WriteStream` is asynchronous, but the observable ordering (chunks written before `end`) is identical.
- Temp files are created with `File::create` (mode from umask); Node uses default stream flags.
- The source never flushes the decoder at the end; the port also drops an incomplete trailing sequence.
