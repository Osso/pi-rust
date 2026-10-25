# Interactive transfer commands

Port frozen `interactive-mode.ts` export/share/copy handlers. [Source and proof](../porting/traceability/interactive-mode.md).

## What it must do

- [x] `/export` uses the shared UTF-16 path parser. A literal lowercase `.jsonl` suffix selects JSONL; all other paths select HTML. Success shows `Session exported to: <path>`. Failure shows `Failed to export session: <Error.message>`, or `Unknown error` for a non-Error rejection.
- [x] `/share` preserves the frozen authentication, temporary HTML export, cancellable loader, child process and gist-result behavior, including exact messages and cleanup. Escape invokes cancellation after loader borrows are released, sends SIGTERM, restores editor focus and removes the temporary file.
- [x] `/copy` preserves the frozen last-assistant-text guard and exact success/error messages. Clipboard work stays delegated to the existing clipboard module.

## Tests asserting this spec

- `ui_tests::{export,copy,share}_commands_match_frozen_frames_and_private_effects` and `oracle/interactive-transfer-oracle.mts`: actual frozen handlers/native processes, full chat/loader frames, JSONL bytes, file cleanup, private fake gh and clipboard effects. Cases still under development are not acceptance evidence.

## Out of scope

Exporter and clipboard algorithms, live providers, public gists and the host clipboard. Tests use private process environments and files only. Timestamped JSONL headers replay the observed native export clock in the frozen oracle; no persisted value is normalized.
