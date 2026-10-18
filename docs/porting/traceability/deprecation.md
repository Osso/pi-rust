# coding-agent deprecation traceability

Pinned source: `packages/coding-agent/src/utils/deprecation.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: the module writes to the process console and keeps global state.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `deprecation.ts:3` | Process-wide set of emitted messages | `emitted_warnings` `OnceLock<Mutex<HashSet<String>>>`; focused dedupe test |
| `deprecation.ts:5-8` | Emit `Deprecation warning: {message}` once, then no-op | `warn_deprecation` / `emit_deprecation`; focused `emits_each_message_once_until_cleared` |
| `deprecation.ts:11-13` | Test-only clear | `clear_deprecation_warnings_for_tests`; same focused test |

## Discrepancies

- The source colors the warning with `chalk.yellow`. The Rust port writes plain text to stderr; terminal styling is not ported.
- `emit_deprecation` is a Rust seam for asserting output without capturing the process stderr; `warn_deprecation` keeps the source behavior.

## Proof inventory

- Focused cases: 1 (dedupe, exact text, clear).
