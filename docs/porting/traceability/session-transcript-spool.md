# session-transcript-spool.ts

## Callback and worker error re-pin (2026-10-25)

`repin_storage_spool::spool_callback_and_worker_errors_match_frozen_cleanup_and_error_values` compile-RED exposed the missing throwable callback API. The current API takes/returns `CallbackError`; real frozen/native probes compare valid decompressed bytes/mode, TypeError+custom code and null callback throws, invalid zstd and truncated-frame worker failures. Private worker output carries the lossless `String(error)` as JSON, matching frozen MessagePort semantics rather than stderr message/newline flattening. The std Read-only boundary converts codec/FS errors once; output open/write errors retain Node records. `SessionManagerError` retains the callback in its source chain and uses the shared `pi_js_compat::callback_error::throw_callback_uncaught` authority for unobserved throws. Existing manager/read callers propagate or catch at their frozen sites.

GREEN log: `/tmp/pi-rust-repin-storage/spool-callback-green-attempt2.log`; initial missing carrier was resolved by fast-forwarding parent `0c700e231` (no local shared-crate edit). Temporary-directory creation/cleanup errors and packaged deployment proofs remain open; no row promotion.

`withSeekableSessionTranscript` maps to `with_seekable_session_transcript`. Existing plain passes through; archives decode into private TempDir with exclusive0600 output, callback, explicit cleanup. Hidden packaged pi subprocess replaces TS Worker: hard90sec deadline, kill+wait on every early exit, contextual error. Sole path config::get_package_dir()/pi(.exe); existing PI_PACKAGE_DIR supports test layout. No thread fallback or SDK registration. Distribution requires packaged pi colocated with package assets; main owns release packaging proof and early CLI dispatch.

Recorded focused timeout/kill, callback/corruption cleanup and large-history proofs: [integration ledger](session-storage-integration.md). Release/SDK package layout remains open. Fork test originally RED on stale plain File::open ENOENT.

Failure-boundary tests: `spool_callback_and_corruption_failures_cleanup_private_files` proves exact decompressed bytes, private0600 output, callback error preserved, no leftover plaintext, corrupt/truncated worker error cleanup. `storage_worker_hard90second_timeout_kills_and_reaps_before_cleanup` blocks real packaged native worker on FIFO, verifies90–100sec deadline and no surviving worker/temp (recorded PASS; see integration ledger). FIFO is failure injection, not a primary transcript/schema fixture.
