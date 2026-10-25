# session-transcript.ts → session_transcript.rs

## Storage filesystem error boundary (2026-10-25)

`repin_storage_transcript::transcript_filesystem_errors_keep_frozen_error_identity_and_fields` compares the actual frozen module for bytes/text/public stream across missing paths, non-directory ancestors, NUL arguments and directory reads (12 cases). Native previously lost Node name/message/code/errno/syscall/path and constructor/stringification; RED/GREEN logs are `/tmp/pi-rust-repin-storage/transcript-fs-{valid-red,green-attempt}.log`. Public creation/whole-file APIs now return `Box<NativeNodeError>`; only `std::io::Read` retains IO errors, translated once by `session_transcript_read_error`. `SessionManagerError` keeps native identity through the shared callback carrier. Spool/history creation callers propagate the typed error. Remaining spool/history IO sites still await exact translation; no whole-module closure.

`repin_storage_transcript::transcript_codec_errors_keep_frozen_error_identity_and_fields` adds actual-source RED/GREEN for synchronous truncated/invalid zstd. The decoder now uses the already-pinned `zstd_safe::DCtx` streaming API so libzstd numeric codes survive the required IO trait instead of reverse-mapping error messages. `session_transcript/errors.rs` maps the pinned libzstd error-code enum names; incomplete input becomes Node `Z_BUF_ERROR`/errno -5/`unexpected end of file`. This changes no dependency. Decoder success/frame/resource regression evidence is recorded below.

## Whole frozen module matrix (2026-10-25)

Frozen 4f9e6f8a4. All five exported functions are covered below; no old-only behavior remains in this module.

| Export/branch | Actual-source behavioral proof |
|---|---|
| `isSessionTranscriptFile`: `.jsonl`, `.jsonl.zst`, other suffixes | `repin_session_storage::frozen_storage_suffix_noops_missing_and_metadata_surviving_plain_branches`, `frozen_transcript_exports_match_stream_frame_and_utf8_error_boundaries` |
| `readSessionTranscriptPath`: existing plain, missing plain/existing archive, neither, already compressed | same suffix/frame matrix; surviving corrupt archive cannot override existing plain |
| `readSessionTranscriptBytes`: plain bytes, archived bytes, filesystem and decompression failures | `repin_storage_transcript` filesystem/codec matrices; reciprocal real TS-compressed/native-compressed fixtures and immutable originals |
| `readSessionTranscriptText`: Buffer UTF-8 decoding and shared errors | frame matrix includes invalid UTF-8; filesystem/codec cases compare error constructor/name/message/String/error properties |
| `createSessionTranscriptReadStream`: plain/compressed, concatenated/skippable frames, non-frame tails, partial UTF-8 reads, source errors | reciprocal frame test reads native output in 13-byte chunks; existing large Unicode resource/history tests; filesystem matrix; approved stream delta tests for late failure |

`repin_storage_transcript::transcript_frames_are_bidirectionally_readable_without_restoration` proves TS `zstdCompressSync` output → native bytes, native frame → frozen public bytes/text/stream, native/skippable/TS concatenation and ignored garbage tails. Original archive bytes remain unchanged and the stale plain path is never restored. Logs: `/tmp/pi-rust-repin-storage/transcript-reciprocal-frames.log`.

At `193f391b`, `repin_storage_transcript` errors PASS2 (12 FS +20 codec cases), `repin_session_storage` PASS31 (unchanged 90-second worker deadline excluded), workspace check JSON0 errors/0 warnings, inventory PASS686/186679, warning clippy0 changed-line findings. Additional reciprocal-frame test PASS1; `approved_transcript_delta` PASS3. Logs: `/tmp/pi-rust-repin-storage/193f391b-*`, `transcript-approved-delta.log`. Spool/history whole-module status is separate; this closure does not promote those rows.

`repin_session_storage::frozen_archive_stale_path_never_allocates_identity_or_masks_history`: public TS manager creates and persists, public archive storage compresses; Rust header/load sees original identity; mutable open refuses stale archive without masking. Initial RED: Rust header None. Recorded focused frame/UTF-8, plain-precedence and resource evidence: [integration ledger](session-storage-integration.md). Whole-module proof remains todo.

Frozen runtime frame boundary: source reader/byte/text APIs accept complete zstd frame followed by non-frame garbage; strict Rust decoder originally rejected Unknown frame descriptor (actual differential RED). SessionZstdDecoder now decodes single frames and inspects only completed-frame successor prefixes, concatenates recognized frames, ignores non-frame tails, and propagates first/in-frame/truncated recognized frame errors. Archive restore/validation share this Node boundary. No general error swallowing or source-bug waiver. Source public streaming truncated-frame unhandled-event crash remains separate classification issue.

## Approved public stream delta 4c8cfc22

`approved_transcript_delta` consumes the public TS stream in a fresh Node child without uncaught-error handlers, using a zstd-compressed Unicode payload repeated 100,000 times and truncated by one compressed byte. Delta child exits 0 and catches exactly Z_BUF_ERROR with no stderr. Frozen child exits 1 with no stdout; full stderr must equal `frozen.normalizedStderr` in [the approved discrepancy record](../deltas/transcript-stream.json). Only numeric locations in the `node:events` header and Node-internal stack frames become `<line>`/`<column>`; exact regexes are recorded there. Function/module names, braces, whitespace, all other lines, final newline and Node version remain contractual. `stderr_normalizer_preserves_unrelated_errors` proves the concrete stderr normalizes exactly, an appended unrelated error survives normalization and fails equality, and a different primary error fails equality. Native public Read stream returns Err after over 1 MB decoded bytes on the same source-created fixture. Native already implements the approved behavior; no production change.
