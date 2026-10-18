# coding-agent exec traceability

Pinned source: `packages/coding-agent/src/core/exec.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: behavioral tests at the real process boundary in `crates/pi-coding-agent/tests/exec.rs` (real `sh` children, real timeouts, real aborts). Differential testing is not applicable: output ordering depends on pipe scheduling.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `ExecOptions` | Optional `signal`, `timeout` (ms), `cwd` | `ExecOptions { signal, timeout }`; `cwd` is a required argument, as in the source signature |
| `ExecResult` | `stdout`, `stderr`, `code`, `killed` | `ExecResult`; `captures_stdout_stderr_and_exit_code` |
| `spawn` with `shell:false`, ignored stdin, piped stdio | Real child in `cwd` | `tokio::process::Command`; `runs_in_the_requested_directory` |
| Spawn error path | `waitForChildProcess` rejects → `{code: 1}` | `failed_exec_result`; `reports_spawn_failures_as_code_one` |
| Abort signal | Already-aborted → immediate SIGTERM; otherwise listener | `start_terminator`; `terminates_immediately_when_the_signal_is_already_aborted`, `terminates_when_the_signal_aborts_during_the_run` |
| Timeout | `timeout > 0` only | `start_terminator` filter; `times_out_and_terminates_the_process`, `ignores_non_positive_timeouts` |
| `killProcess` | SIGTERM, mark `killed`; SIGKILL after 5 s only if SIGTERM failed | `terminate` (escalation guarded by the SIGTERM result, mirroring `!proc.killed`) |
| `code ?? 0` | Signal exit reports code 0 | `status.code().unwrap_or(0)`; timeout test asserts code 0 |
| Output accumulation | Concatenated stdout/stderr | `wait_for_child_process` drains both pipes; UTF-8 decoding is lossy |

## Exclusions

- `killed` is an `AtomicBool` shared with the terminator task instead of a closure variable.

## Discrepancies

- The source decodes each chunk with `data.toString()`; a multi-byte character split across chunk boundaries becomes U+FFFD. The Rust port decodes the full stream, so it never splits a character. This is deterministic and strictly more faithful to the bytes; `decodes_invalid_utf8_lossily` pins the invalid-byte behavior.
- The source's SIGKILL escalation is unreachable in practice (`proc.killed` is already true after a successful SIGTERM); the Rust port keeps the same guard.
