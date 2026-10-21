# session-delete traceability

Frozen source: `packages/coding-agent/src/core/session-delete.ts` at `4f9e6f8a4` ([lock](../source-lock.json)).
Rust: `crates/pi-coding-agent/src/core/session_delete.rs`.

| Source | Rust | Proof |
|---|---|---|
| `deleteSessionTree` | `delete_session_tree`, `delete_tree_files` | `tests/session_delete.rs`; RED observed before implementation; GREEN pending |
| `chunkSessionPaths` | `chunk_session_paths` | batch-budget fixture pending |
| `deleteSessionBatch` | `delete_session_batch` | standalone, absent file, failed trash/unlink fixture |
| `trashSessionFiles` | `trash_session_files` | isolated fake trash and missing binary |
| descendant metadata cleanup, partial failures | control DB APIs | pending tree fixtures |

No live state used. Error/platform edge coverage is not yet whole-module proof.
