# coding-agent output-guard traceability

Pinned source: `packages/coding-agent/src/core/output-guard.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: unit tests for the chunk splitter inside `crates/pi-coding-agent/src/core/output_guard.rs`, plus a child-process test (`crates/pi-coding-agent/tests/output_guard.rs`) that writes through the real writer and asserts byte order and takeover-flag transitions.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `RAW_STDOUT_MAX_CHUNK_LENGTH` | 1024 | `RAW_STDOUT_MAX_CHUNK_LENGTH` |
| `RAW_STDOUT_RETRY_DELAY_MS` | 10 | `RAW_STDOUT_RETRY_DELAY` |
| `splitRawStdoutText` | Newline-terminated chunks, capped at 1024 UTF-16 units | `split_raw_stdout_text`; `splits_complete_lines_as_separate_chunks`, `splits_long_lines_at_1024_units`, `prefers_a_newline_inside_the_chunk_window`, `cuts_astral_characters_at_the_utf16_boundary_like_slice` |
| `writeRawStdout` | Empty text ignored; chunks queued in order | `write_raw_stdout`; probe test asserts order and no duplication |
| `writeRawStdoutChunk` retry | `ENOBUFS`/`EAGAIN`/`EWOULDBLOCK` retried after 10 ms | `write_chunk_with_retry`/`is_retryable` |
| Writer failure | `process.exit(1)` | `std::process::exit(1)` (unreachable with a healthy stdout) |
| `waitForRawStdoutBackpressure` | Resolves when the queue drains | `wait_for_raw_stdout_backpressure` over a `watch` counter; probe test |
| `flushRawStdout` | Waits for backpressure, then writes an empty chunk | `flush_raw_stdout`; probe test |
| `takeOverStdout`/`restoreStdout`/`isStdoutTakenOver` | Process-wide takeover flag | `take_over_stdout`/`restore_stdout`/`is_stdout_taken_over`; probe asserts `true,false,false` |

## Approved deviation (user decision 2026-10-18)

The source replaces `process.stdout.write` so stray writes are rerouted to stderr. Rust cannot intercept `println!`, so the port keeps the writer, queue, backpressure, and the takeover flag, and prevents stray stdout writes statically with `#![deny(clippy::print_stdout)]` at the crate root. No file-descriptor redirection happens.

## Exclusions

- The writer owns a dedicated thread and an ordered channel instead of a promise chain.
- `takeOverStdout` is idempotent and `restoreStdout` is a no-op when not taken over; both end in the same flag state as the source.

## Discrepancies

- Astral characters cut at the 1024-unit boundary become a lone surrogate in the source, which Node encodes as U+FFFD; the Rust port converts with `from_utf16_lossy`, producing the same replacement character.
