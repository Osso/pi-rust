# session-delete traceability

Frozen source: `packages/coding-agent/src/core/session-delete.ts` at `4f9e6f8a4` ([lock](../source-lock.json)). Rust: `crates/pi-coding-agent-core/src/core/session_delete.rs`. Contract: [spec](../../specs/session-archive.md).

| Source symbol/branch | Rust | Frozen differential / behavioral proof |
|---|---|---|
| `deleteSessionTree`, descendant-before-root, archived aliases | `delete_session_tree`, `delete_tree_files` | `session_delete::frozen_tree_deletion_batches_long_paths_and_cleans_partial_metadata`: nested .zst descendants, missing file, unrelated preservation |
| `chunkSessionPaths`, UTF-8 bytes + argv overhead, 64 KiB | `chunk_session_paths` | same test: 700 long multibyte paths; fake trash rejects oversize argv, source/native both report trash (not unlink) |
| `deleteSessionBatch`, partial trash, failing unlink | `delete_session_batch` | partial directory fixture compares exact error and retained/removed metadata against frozen |
| cleanup of only removed paths; cleanup error replaces result | `delete_session_tree` | same test, SQLite abort trigger compares exact error and retained metadata |
| `trashSessionFiles`, unavailable executable, standalone/no DB, already missing | `trash_session_files` | `deletion_matches_frozen_for_trash_unlink_missing_and_partial_failure` |
| spawn EACCES, exit code, signal, 200-unit stderr/first line, successful trash leaving files | diagnostic helpers | `trash_error_messages_match_frozen`: exact source/native result JSON |
| execvp ENOEXEC executable-text handling | `spawn_trash` | `trash_without_shebang_matches_node_execution`: frozen Node returns `trash exited with 7`; native matches, including `$0`, literal `--` and metacharacter-containing path |
| UTF-16 slice splitting a surrogate | `format_trash_error`, `trim_utf16`, public `JsString` error, lossless JSON serializer | `trash_split_surrogate_preserves_public_utf16`: exact frozen public units and JSON string, trailing `[120,55357,41]` |

RED: initial no-op deletion left the file; later spawn-permission fixture exposed native OS wording instead of source `spawn trash EACCES`. Both fixed. All four target tests GREEN at `3d4e6a32`; `delete_worker` is a subprocess entry point, not an additional behavior case. Source/native process invocations stay inside Cargo's mandatory sandbox, private PATH and temporary DB/files only.

Mutant skipping nonempty metadata cleanup killed by retained-tree metadata assertion. [Report](../session-archive-history-proof.md) records revisions and remaining verification limits. Verifier corrections at `9bb38edf`: six deletion target tests pass, including the worker entry point. Both executable-text and surrogate cases reproduced RED; ENOEXEC mutant killed. [Updated proof](../session-archive-history-proof.md#verifier-corrections-2026-10-21). Public diagnostics retain UTF-16 through assembly and direct JSON serialization. Final extension stderr explicitly encodes with Node-style replacement; `serde_json::Value` cannot represent lone surrogates and rejects that conversion rather than silently replacing. Other-platform process diagnostics and pathological invalid-UTF-8 stream decoding remain unproven.
