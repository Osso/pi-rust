# Coding-agent output-accumulator traceability

Pinned source: `packages/coding-agent/src/core/tools/output-accumulator.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::output_accumulator`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `output-accumulator.ts:10-23` | `OutputAccumulatorOptions` and `OutputSnapshot` records; `fullOutputPath` is unset until a temp file exists | `OutputAccumulatorOptions`, `OutputSnapshot`; focused temp-file tests and differential `hasFullOutputPath` |
| `output-accumulator.ts:25-28` | Random temp path from `randomBytes(8)` under `os.tmpdir()` | `default_temp_file_path`; `new_with_temp_file_path` injects a deterministic path for tests and the oracle |
| `output-accumulator.ts:34-48` | `maxRollingBytes = max(maxBytes * 2, 1)`; defaults 2,000 lines / 10 KiB / `pi-output` prefix | Constructor; focused rolling-tail test |
| `output-accumulator.ts:57-72` | `append` rejects post-finish calls, counts raw bytes, decodes streaming UTF-8, then persists or buffers raw chunks | `append` (panics with the source message); focused panic test and differential `appendError` |
| `output-accumulator.ts:74-84` | `finish` is idempotent, flushes the decoder, and opens the temp file when over a limit | `finish`; focused idempotence/flush test |
| `output-accumulator.ts:86-117` | `snapshot` tail-truncates the rolling text, overrides `truncated`/`truncatedBy`/totals from accumulator counters, and persists when asked | `snapshot`; focused truncation tests and differential full-truncation comparison |
| `output-accumulator.ts:119-147` | `closeTempFile` flushes and clears the stream, rejecting on stream error | `close_temp_file` flushes and drops the handle; errors are returned |
| `output-accumulator.ts:149-151` | `getLastLineBytes` | `last_line_bytes`; differential per-chunk values |
| `output-accumulator.ts:153-183` | Decoded text accounting: totals, rolling tail, newline counting, open-line byte count | `append_decoded_text`; focused line tests and differential `lastLineBytes`/`totalBytes` |
| `output-accumulator.ts:185-201` | `trimTail` cuts on a UTF-8 lead byte and tracks whether the tail starts at a line boundary | `trim_tail`; focused rolling-tail test |
| `output-accumulator.ts:203-211` | `getSnapshotText` drops a partial first line when the tail starts mid-line | `snapshot_text`; differential rolling-tail corpus |
| `output-accumulator.ts:213-226` | `shouldUseTempFile` compares raw bytes, decoded bytes, and lines against the limits | `should_use_temp_file`; focused temp-file tests |
| `output-accumulator.ts:228-240` | `ensureTempFile` creates the stream once and replays buffered raw chunks | `ensure_temp_file`; focused replay test and differential `tempFileHex` |
| `new TextDecoder()` streaming decode | Buffers incomplete sequences, one replacement per invalid prefix, strips a leading BOM at stream start | `Utf8StreamDecoder`; unit tests, focused BOM tests, and 120 seeded differential chunk sequences including invalid, overlong, surrogate, and out-of-range bytes |

Differential: `oracle/coding-agent-output-accumulator-oracle.ts` drives the real class (request `27003`, 139 cases), comparing content, full truncation records, last-line byte counts, temp-file bytes, and the post-finish error message.

## Discrepancy avoided

Node's streaming `TextDecoder` consumes the whole invalid prefix (lead plus valid continuations) for one replacement character, and it strips a leading BOM even when the three bytes arrive in different chunks. The Rust decoder reproduces both rules; the differential corpus fails otherwise.

## Deviations

- `append` panics instead of throwing; the message is byte-identical.
- Temp-file writes are synchronous (`File`), so `close_temp_file` only flushes. The source's async stream error path has no Rust equivalent because the write cannot be observed asynchronously here.
- Rust strings cannot hold lone surrogates, so decoded text always contains valid scalars, as with every other Rust boundary in this crate.
