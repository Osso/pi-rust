# coding-agent deprecation traceability

Pinned source: `packages/coding-agent/src/utils/deprecation.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: the module writes to the process console and keeps global state.
The chalk dependency it uses is proven separately in [chalk.md](chalk.md).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `deprecation.ts:3` | Process-wide set of emitted messages | `emitted_warnings` `OnceLock<Mutex<HashSet<String>>>`; focused dedupe test |
| `deprecation.ts:5-8` | Emit `Deprecation warning: {message}` once, then no-op | `warn_deprecation` / `emit_deprecation_with`; focused `emits_each_message_once_until_cleared` |
| `deprecation.ts:8` | `console.warn` writes the chalk-yellow message to stderr with a trailing newline | `warn_deprecation` uses `Chalk::stderr()`; `emit_deprecation_with` writes the styled line; focused `emits_each_message_once_until_cleared` (yellow phase) |
| `deprecation.ts:11-13` | Test-only clear | `clear_deprecation_warnings_for_tests`; same focused test |

## Discrepancies

- `emit_deprecation_with` is a Rust seam for asserting output with an injected color level instead
  of the process stderr; `warn_deprecation` keeps the source behavior.

## Proof inventory

- Focused cases: 1 test with three phases (dedupe/exact plain text/clear, then yellow SGR codes
  at level 1). The phases share one test because the emitted-message set is process-wide.
