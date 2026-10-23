# Ls tool

Match `coding-agent/src/core/tools/ls.ts` at immutable4f9e6f8a4. [Traceability](../porting/traceability/ls.md).

## Numeric limits
- [x] Retain optional binary64 limits, including fractions, signed zero, NaN and infinities; absence uses500.
- [x] Filesystem listing compares successful entry count directly to the number before the next stat; skipped stat failures do not count. Empty results return the frozen empty-directory result without a notice.
- [x] RTK listing applies Array.slice conversion, including negative relative indices and NaN→0; its entry-limit flag independently compares line count to the original number.
- [x] Preserve original numeric details and JavaScript spelling of limit/doubled-limit notices; JSON wire serializes non-finite numbers as null.
- [x] Use the shared [slice-index authority](js-slice-index.md), not integer coercion/defaulting.

## Rendering
- [x] Reuse the prior Text component in each call/result slot; update its text without stale output.
- [x] Match path/default and numeric call labels, exact collapsed/expanded output/hints, text/image projection and trimming.
- [x] Apply source warning truthiness: zero/NaN omit entry-limit warning; other numeric values retain original spelling. Join byte/entry warnings in source order.

## Remaining module cycle
- [x] Exact local/injected IO and error ordering.
- [x] [First-winner abort observer](tool-promise-observer.md) rejects promptly without cancelling retained producer work or skipping later operations.
- [x] RTK validates file/argv before cwd through shared Node string validation; preserve NUL diagnostics and ordering before native encoding/spawn.
- [x] Local filesystem errors preserve Node `scandir` messages through the shared native-error carrier until the outer `Cannot read directory: ` boundary. Real files prove EACCES, post-stat ENOENT/ENOTDIR, post-exists stat ENOENT, skipped broken/cyclic/invalid-UTF-8 entries and directory symlink suffixes.
- [x] Ordinary RTK stdout/stderr preference, JS trimming, command-failure text and missing-executable behavior.
- [x] [Shared bounded capture](child-process-capture.md) caps both streams and terminates early with UTF-8-safe output; chunk-dependent Node overflow prefix is an explicit runtime gap.
- [x] Whole current-pin symbol/branch map; typed invalid-runtime exclusions and approved overflow-prefix gap explicit.

## Proof
`differential_ls_numbers`:17 actual frozen injected-operation cases plus17 real fake-RTK process cases. Exact traces prove limit checks happen before stat and enough entries remain for a second page. `ls_tool` covers ordinary sorting, dotfiles, directories, limits and byte truncation.

`differential_ls_filesystem` adds8 actual-frozen real-file cases at exists/stat/readdir boundaries. `differential_ls_rtk_io` covers15 ordinary/error/validation/truncation/overflow-label cases plus native both-stream termination safety. Renderer16-frame and abort9-history differentials preserve built-in state/lifetime behavior. Final gate results/revisions live in the proof ledger.

No new dependency or numeric deviation.
