# Resume session tool

Match entire `packages/coding-agent/src/core/tools/resume-session.ts` at immutable `4f9e6f8a4`.

- Sequential main-session tool. Reject nonempty child agent IDs or required-agent-ID contexts before validation; empty ID alone is falsy. Normalize exactly one path, ID or name, trimming each string and optional starter_prompt. Explicit null/blank/wrong fields reject in frozen validation order.
- Resolve direct paths relative to context cwd. Select the archive-aware transcript read path before existence/stat checks. Require a file, then accept `.jsonl` or `isArchivedSessionFile`.
- DISCREPANCY: that archive predicate accepts any `.zst` suffix, although the error says `Session file must be a .jsonl or .jsonl.zst file: {path}`. Preserve both literal acceptance and wording; this tool does not decode transcripts.
- ID/name selection follows frozen scoped metadata, all metadata, scoped scans and default scans ordering; exact ID precedes prefix. Preserve path-keyed last-value/first-position merging and ambiguity errors.
- Reject the current resolved session path. Call switchSession and optionally send the trimmed original UTF-16 starter prompt through its replacement context. Reflect cancelled/resumed state in complete terminating results.
- Runtime transition/storage/DB internals remain their owners' responsibility. This tool preserves native UTF-16 values until actual filesystem/storage boundaries.

Missing custom-JS switchSession is a typed-context exclusion accepted2026-10-24: frozen runner.ts:946-949 and native ExtensionContext always provide the method. [Traceability](../porting/traceability/resume-session.md) tracks proofs and remaining work.
