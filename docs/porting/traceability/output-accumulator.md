# Output accumulator — current-pin prerequisite

Frozen `packages/coding-agent/src/core/tools/output-accumulator.ts` at `4f9e6f8a4`. This records the cleared temp-path prerequisite, not whole-module parity.

| Frozen branch | Native behavior | Proof |
|---|---|---|
| `defaultTempFilePath`, line21: `join(tmpdir(), ...)` | [Shared POSIX getter](../../specs/node-tmpdir.md) | Actual Node getter11; frozen consumer histories below |
| Constructor57–61 stores limits/prefix only; ensureTempFile217–219 computes the first path | Default path generated at first spill, not construction; existing native explicit-path constructor remains available | `differential_tmpdir_consumers::accumulator_reads_tmpdir_at_first_spill_not_construction`, four mutable TMPDIR/TMP/TEMP/trailing-slash cases |
| ensureTempFile215–219 guards `tempFilePath`, not the live stream | Keep first path after close; persisting another snapshot neither reopens nor truncates the file | Same histories close the real file, persist a second snapshot and compare full bytes/path |

Valid `tmpdir-accumulator-red-2.log` proves wrong constructor-time root and empty full-file bytes after post-close snapshot, against actual frozen output (`payload`). Initial `tmpdir-accumulator-red.log` was only a test import compile failure and is not behavioral evidence. Production correction follows the source predicate; no new queue, hook or fallback added. GREEN at e5155f38: four histories, `tmpdir-accumulator-green.log`. Twelve combined consumer cases plus retained core/wrapper histories pass at ea60354a (`tmpdir-consumers-green.log`); [Integration checkpoint](node-os.md#tmpdir-checkpoint-eabd39b5).

Numeric, retained decoder/byte corpus and artifact-format proofs follow below. Whole exported lifecycle and stream-error delivery audit remains open; this prerequisite does not close the coverage row.

## Current-pin baseline audit

`accumulator-current-baseline.log` initially fails solely on the test's serde_json integer-versus-float representation (2000 versus2000.0), not accumulator values. The fixture now additionally compares exact `JSON.stringify(snapshot.truncation)` bytes against the production lossless metadata serialization authority. With that assertion retained, JavaScript-value comparison passes all139 existing histories (`accumulator-current-byte-audit.log`). No production writer change or relaxed field assertion. At that baseline, numeric limits outside usize, entropy and actual stream error delivery remained open.

Numeric audit: frozen12 explicit negative/fractional/signed-zero/nonfinite/large limit cases pass (`accumulator-numeric-source-contract.log`). Native API compile RED (`accumulator-numeric-api-red.log`) establishes the former usize narrowing; options/rolling arithmetic now use f64 and shared JS Math.max/slice-index authorities. Existing139 histories remain GREEN. Full numeric differential first RED at shared truncateStringToBytesFromEnd(…,1.25): native returned one suffix byte, source two. Parent cleared the shared prerequisite; direct frozen RED established it independently (`accumulator-shared-truncate-direct-red.log`). The helper now subtracts raw doubles, preserves fractional Buffer-property semantics and coerces only at slice; the other narrowed helper, truncateLine, now accepts f64 (source contract + API RED). All1741 direct truncate cases,139 retained accumulator cases and12 full numeric accumulator histories pass (`accumulator-numeric-shared-green.log`). [Truncation map](coding-agent-truncate.md), [contract](../../specs/output-accumulator.md). Source fractional-byte overrun/replacement behavior is recorded, not silently corrected.

Default suffix audit: frozen calls crypto.randomBytes(8); native formerly used timestamp XOR counter. Native now uses the existing shared getrandom OS authority and big-endian lowercase hex formatting. Per parent instruction, `default_and_custom_artifact_names_match_the_frozen_crypto_hex_format` asserts format only (default/custom prefix, sixteen lowercase hex digits, original disk bytes); it is not labeled entropy RED. Source/native format contract GREEN (`accumulator-artifact-format-contract.log`); no RNG injection or new dependency.

The former sync-IO classification below was a pre-repin main ruling, not user approval. Re-derive from frozen stream listeners and real faults; do not treat it as current-pin acceptance.

## Real stream-fault boundary RED

`differential_accumulator_open_errors` runs frozen/native processes against real EACCES and the existing descriptor-only write/close fault fixture. The source must first demonstrate each expected event, exit, error code and syscall; no generic startup failure counts as evidence. `accumulator-stream-seven-faults-red-2.log` proves seven mismatches:

- Open, first write and replayed-buffer write: append returns. With no stream listener, error is uncaught; with close awaiting the stream, error rejects close. Native currently exits during append in all six cases.
- Descriptor close failure: frozen close resolves on finish, removes its error listener, then EBADF is uncaught. Native currently rejects close instead.

The first seven-fault attempt incorrectly predicted syscall writev for the replayed-buffer case; actual source reports write. That assertion was corrected from the observed source, without changing production. The pre-repin synchronous classification is disproven. Parent approved a claimable Mutex<Option<Box<NativeNodeError>>> slot and one task on the captured owning Tokio runtime: yield, then atomically take the error. Close's synchronous prefix takes the same slot; stream finish clears the listener before descriptor-close errors are deferred. No queue/timer/framework. The public accumulator finish method only flushes its decoder; it is not the write stream finish event.

All seven event/exit histories and original error fields now match (`accumulator-stream-seven-metadata-green.log`). Only nondeterministic artifact names are normalized, after asserting actual source codes/syscalls and artifact paths. Source/native error name/message/code/errno/syscall/path remain compared. The old production close-syscall function-pointer test seam and its incorrect close-rejection unit were removed; real close-fault process proof replaces them. Native close returns the original boxed NativeNodeError; Bash preserves its message rather than relabeling an open/write failure as close.

The same shared yielding slot authority replaces executor spill's raw spawned thread. The executor captures its owning runtime so a retained callback invoked outside its entered scope still dispatches there. Deferred accumulator events similarly capture the entered runtime at construction. Focused executor regression remains required for this shared change.

## Retained historical map (not current-pin acceptance)

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `output-accumulator.ts:10-23` | `OutputAccumulatorOptions` and `OutputSnapshot` records; `fullOutputPath` is unset until a temp file exists | `OutputAccumulatorOptions`, `OutputSnapshot`; focused temp-file tests and differential `hasFullOutputPath` |
| `output-accumulator.ts:25-28` | Random temp path from `randomBytes(8)` under `os.tmpdir()` | `default_temp_file_path`; `new_with_temp_file_path` injects a deterministic path for tests and the oracle |
| `output-accumulator.ts:34-48` | `maxRollingBytes = max(maxBytes * 2, 1)`; defaults 2,000 lines / 10 KiB / `pi-output` prefix | Constructor; focused rolling-tail test |
| `output-accumulator.ts:57-72` | `append` rejects post-finish calls, counts raw bytes, decodes streaming UTF-8, then persists or buffers raw chunks | `append` (panics with the source message); focused panic test and differential `appendError` |
| `output-accumulator.ts:74-84` | `finish` is idempotent, flushes the decoder, and opens the temp file when over a limit | `finish`; focused idempotence/flush test |
| `output-accumulator.ts:86-117` | `snapshot` tail-truncates the rolling text, overrides `truncated`/`truncatedBy`/totals from accumulator counters, and persists when asked | `snapshot`; focused truncation tests and differential full-truncation comparison |
| `output-accumulator.ts:119-147` | `closeTempFile` flushes and clears the stream, rejecting on stream error | `close_temp_file` flushes and explicitly closes via nix; flush/close errors are returned |
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

## Historical deviations (not current-pin approval)

- `append` panics instead of throwing; the message is byte-identical.
- Main-authorized timing classification (2026-10-21): Rust keeps synchronous IO. Create/write failures before close route through `throw_uncaught`, using Node filesystem diagnostics and the installed process crash handler. Flush/close failures return `Err`. Node's async write error may arrive after close installs its listener and therefore reject close rather than crash; Rust classifies by the operation detecting the error. No errors are latched or suppressed.
- Snapshot text, retained tail, prefix and paths now use JsString. Scalar decoding exists only inside the actual external UTF-8 decoder; snapshot construction no longer checks/narrows the native text.

## Q18 current proof

[Native fields and checkpoint ledger](lossless-tool-paths.md#output-accumulator-checkpoint). `lossless_output_accumulator` proves raw path retention and actual disk bytes. Subprocess EACCES and ENOSPC fixtures prove create/write failures exit 1 through the installed uncaught handler with exact Node-style messages. A real-file test injects only the close syscall's EIO result (deterministic kernel close errors otherwise require invalid FD ownership or an unreliable filesystem), proving returned error behavior rather than source shape. The139-case corpus and focused consumers are now revalidated above. Historical uncaught tests do not establish the awaiting-close boundary; the new real-fault RED supersedes that classification.
