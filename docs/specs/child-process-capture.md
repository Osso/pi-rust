# Child process output capture

Shared `utils::child_process::capture_child_process` extends the existing wait/read authority; no second output-reader implementation.

- [x] Preserve existing unbounded wait behavior when no cap/signal is configured, including idle grace for inherited pipes.
- [x] Apply a configured byte cap independently to stdout/stderr; retain bounded bytes and terminate/reap immediately on overflow or abort.
- [x] Stop output readers after completion and report read failures explicitly rather than treating them as successful EOF.
- [x] Expose the stop reason separately from captured status/bytes. RTK retains frozen stdout/stderr maxBuffer labels and ordinary command-error formatting.
- [x] RTK decodes output safely and clamps at a UTF-8 boundary, including replacement expansion; no byte-index panic.

## Runtime gap
Node execFile's chunk-dependent UTF-16 overflow prefix is not reproduced. User approval2026-10-23: sensible bounded native capture, exact configured cap/ordinary output/error labels, explicit prefix gap. This replaces the old post-read approximation; both streams are now bounded before producer completion.

## Proof
`differential_ls_rtk_io` compares seven actual frozen success/failure outputs, including BOM/Unicode whitespace and quiet nonzero exit. Separate native safety cases write multi-megabyte UTF-8 to each stream; error output stays within20KiB and producer completion markers never appear. RED reproduces UTF-8 slicing panic; `ls-rtk-io-green-2.log`9/0 with six shared child-process and existing RTK controls.
