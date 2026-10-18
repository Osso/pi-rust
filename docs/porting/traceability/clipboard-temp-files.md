# coding-agent clipboard-temp-files traceability

Pinned source: `packages/coding-agent/src/utils/clipboard-temp-files.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential test skipped: behavior is filesystem state; proven behaviorally in `crates/pi-coding-agent/tests/clipboard_temp_files.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `clipboard-temp-files.ts:3-7` | Tracker exposes `track`, `cleanupReferencedIn`, `cleanupAll` | `utils::clipboard_temp_files::ClipboardTempFileTracker`; focused tests |
| `clipboard-temp-files.ts:9-14` | File removal is best effort | `remove_file` ignores errors; focused `tolerates_files_that_already_disappeared` |
| `clipboard-temp-files.ts:17-19` | Track paths in a set | `track`; focused reference and cleanup-all tests |
| `clipboard-temp-files.ts:21-29` | Remove tracked files whose path appears in text, then forget them | `cleanup_referenced_in`; focused `deletes_tracked_files_referenced_by_submitted_text`, `keeps_tracked_files_until_text_references_them` |
| `clipboard-temp-files.ts:31-36` | Remove and forget every tracked file | `cleanup_all`; focused cleanup-all test |

## Discrepancies

- Paths are `PathBuf` instead of strings; matching uses the lossy string form.

## Proof inventory

- Focused cases: 4.
