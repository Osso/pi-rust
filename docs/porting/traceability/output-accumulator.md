# Output accumulator — common-pin map

Source: entire `packages/coding-agent/src/core/tools/output-accumulator.ts` at immutable `4f9e6f8a4`. Native: `core/tools/output_accumulator.rs`; [contract](../../specs/output-accumulator.md). Implementation and focused proofs below complete the audit; final checkpoint gates remain pending. This is not a workspace release claim.

## Exact API and branch matrix

| Source symbol/branch | Native representation | Behavioral proof |
|---|---|---|
| OutputAccumulatorOptions: maxLines/maxBytes nullish defaults; tempFilePrefix | f64 limits, native UTF-16 prefix; default constants | Retained139 histories;12 explicit negative/fraction/±0/NaN/±Infinity/large-number histories; format/default/raw-prefix cases |
| OutputSnapshot: content/truncation/fullOutputPath | JsString text/path, full TruncationResult, optional first path | Full values plus exact JSON.stringify metadata bytes in every ordinary/numeric/sequence snapshot |
| Constructor stores limits/prefix; maxRollingBytes=Math.max(maxBytes*2,1) | Shared JS Math.max; no eager path generation |12 number histories; mutable tmpdir first-spill4; untouched/negative sequence |
| append: finished guard, raw byte accounting, streaming decode, decoded accounting, spill or raw-buffer retention; skip empty raw chunks | Shared TextDecoder authority, owned byte chunks, source-order updates |139 corpus histories; incomplete UTF8, BOM/raw-only spill, empty append, post-finish rejection and intermediate snapshots in sequence histories |
| finish: idempotent; flush decoder; spill original raw chunks if decoded limit now exceeded | finished flag and unchanged raw replay; not a write-stream finish event | Incomplete two-byte input spills only after decoder flush, disk remains original two bytes; repeated finish/late append sequence |
| snapshot: tail text/truncateTail, global count/limit overrides, truncatedBy fallback, optional persistence | Shared exact [truncate](coding-agent-truncate.md) and JS-number limits |139+12 full metadata/JSON histories; untouched negative limits persist only when requested; same path after close |
| closeTempFile: absent stream no-op; clear reference; listen to error/finish; end | File option, stream-presence flag, claimable native-error slot | Close-before-spill, repeated close, append-after-close sequences; nine real-fault histories below |
| getLastLineBytes | Current decoded unterminated-line UTF8 byte count | Per-chunk and after-finish checks; split euro, invalid/incomplete/BOM input, newline/open-line transitions |
| appendDecodedText: empty fast path, UTF8 counts, rolling threshold, newline count/last newline/open line | JsString tail with shared Node byte encoding |139 corpus plus intermediate rolling snapshots and decoder-flush sequence |
| trimTail: raw double subtraction, fractional Buffer property, continuation-byte skip, boundary tracking, slice coercion | Shared JS slice index; Buffer decoding only at its actual boundary | Integer/multibyte/fractional numeric and rolling histories. Private start=0/preserved-boundary and buffer<=rolling guards are defensive: public trimming starts only after exact tail bytes exceed twice the immutable rolling bound |
| getSnapshotText: intact boundary, partial first line with/without newline | Exact UTF16 LF search/slice | Rolling sequence and retained corpus |
| shouldUseTempFile: raw OR decoded bytes OR lines | Source comparisons against f64 limits | Raw-only BOM spill, decoder-flush-only spill, line threshold, byte threshold and numeric histories |
| ensureTempFile: first-path guard, lazy random path, stream construction, replay then new bytes | Original path retained; synchronous path validation before stream-presence flag; native file plus deferred errors | Mutable tmpdir4, default/custom/raw-prefix naming, complete disk hex, retained-path/append-after-close sequence; sync-path and stream-fault proofs |
| defaultTempFilePath/byteLength | Node join/tmpdir; eight OS RNG bytes, lowercase hex; Node UTF8 encoder | Default/custom artifact format and raw-D800 prefix with actual disk bytes; no timestamp/counter suffix |

Source's sole built-in consumer is bash.ts:626,678–681: it passes each operation buffer to append without mutating it. Its finishOutput finishes decoding, snapshots, then awaits close. Native owned buffer copies follow the recorded object-identity policy; no built-in alias mutation is required. Snapshot values are owned, not mutable references into later state.

## Numeric and byte proof

- Initial `accumulator-current-baseline.log` failure was the test's serde_json integer-versus-float distinction, not a writer bug. The fixture first added exact source JSON.stringify metadata bytes against the production lossless serialization authority; only then compared JavaScript values. `accumulator-current-byte-audit.log` GREEN139, no field assertion removed.
- `accumulator-numeric-source-contract.log` GREEN12 before the native f64 API change; `accumulator-numeric-api-red.log` records the former usize-only compile boundary.
- A real numeric differential exposed the shared suffix helper flooring1.25 too early. Parent cleared the prerequisite. Direct source contract, direct RED ('t' versus 'st') and line-limit API RED precede the shared repair. All1741 direct truncate cases plus139+12 accumulator histories GREEN in `accumulator-numeric-shared-green.log`.
- DISCREPANCY retained: fractional Buffer indexes are undefined; only slice floors. A fractional byte budget can therefore retain too many bytes or decode continuation bytes as U+FFFD. Native follows frozen, not its UTF8-safe comment.

## Stream listeners and real faults

Pre-repin synchronous-IO classification was a main ruling, not user approval. `accumulator-stream-seven-faults-red-2.log` disproves it at the current pin:

- Open EACCES, first write EIO and buffered replay write EIO: append returns. No stream listener means uncaught; an awaiting close listener rejects close. Native formerly exited during append in all six cases.
- Descriptor close EBADF occurs after the stream finish event resolves close and removes its listener: uncaught, not a close rejection.

Parent-approved model: Mutex<Option<Box<NativeNodeError>>> slot; close's synchronous prefix claims it. A task on the captured owning Tokio runtime first yields, then atomically takes any unclaimed error through shared uncaught authority. Stream finish clears the listener before descriptor-close errors enter that slot. No queue, delay timer or framework. Public accumulator finish only flushes its decoder. Production `bin/pi.rs` and the probes use current-thread Tokio.

`accumulator-stream-seven-metadata-green.log` matches all seven event/exit histories and original error fields. `accumulator-stream-nine-faults-green.log` adds combined write+close precedence, with/without close listener, and a second close that must not absorb an already-finished stream's unhandled descriptor-close error. Existing descriptor-only C fixture affects real file syscalls, never protocol pipes. Source must demonstrate actual error code/syscall first; only random artifact paths are normalized. Error name/message/code/errno/syscall/path remain compared.

Initial replay fixture predicted syscall writev; observed frozen reports write, so the test was corrected rather than production altered. The old production close-syscall function-pointer seam and incorrect close-rejection unit were removed; process fault proof replaces them.

The shared yielding-slot authority also replaces executor spill's raw spawned thread. Captured runtime ownership preserves retained callbacks invoked outside its entered scope. `accumulator-deferred-retained.log` GREEN executor19, accumulator139+12/artifact format, and existing uncaught/raw-path consumers. Bash now forwards the original close rejection's message, never relabeling an open/write error as close.

## Synchronous validation and lifecycle

`accumulator-path-sync-red.log` proves NUL paths incorrectly deferred EINVAL. Shared Node path validation now throws original TypeError/ERR_INVALID_ARG_VALUE before a stream exists, retaining the computed raw path. `accumulator-path-sync-green.log` compares original error, snapshot, exact metadata JSON and successful close after rejected construction. No retry or OS path coercion.

`differential_accumulator_sequences` exercises intermediate snapshots, decoder-flush-only spill, BOM/raw-only spill, repeated finish, post-finish append rejection, close-before-spill, append-after-close without reopening, untouched negative limits, empty append and partial-line rolling. It compares complete metadata, exact metadata bytes, last-line bytes, path retention and full disk hex after close. Initial six histories GREEN `accumulator-sequence-green.log`; expanded seven-history proof follows at final checkpoint.

## Path and entropy

[Shared POSIX tmpdir](../../specs/node-tmpdir.md) and first-spill4 prove lazy directory selection and first-path retention after close. Initial import-only failure is not behavioral RED; valid `tmpdir-accumulator-red-2.log` precedes e5155f38 correction. Twelve combined directory consumers passed at ea60354a.

Frozen uses crypto.randomBytes(8); native timestamp XOR counter was replaced by existing getrandom OS authority, big-endian sixteen-digit lowercase hex. Parent requested format-only assertions: no RNG injection, entropy-shaped test or new dependency. Default/custom prefix format and original file bytes pass; default raw-D800 prefix retains exact UTF16 through snapshot and repairs only at the actual OS boundary (`accumulator-default-raw-prefix-green-2.log`). Initial raw-prefix attempt was a test import/constructor compile error, not behavioral evidence.
