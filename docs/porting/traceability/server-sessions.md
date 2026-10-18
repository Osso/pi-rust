# server/sessions.ts traceability

Source: `packages/server/src/sessions.ts`. Rust: `crates/pi-server/src/sessions.rs`.

| TS item | Rust | Proof |
|---|---|---|
| `LiveSessionManager` | `LiveSessionManager` | `tests/sessions.rs` (12 tests) |
| `executeCommand` list/create/attach/detach/prompt/steer/abort/set_model/set_thinking | `execute` + command dispatch | `tests/sessions.rs`, `tests/conformance.rs` |
| `disconnect` | `disconnect` | busy-work/terminal-error tests |
| `listMetadata` | `list_metadata_inner` | metadata merge/restart tests |
| `close` | `close` | graceful close conformance test |
| `acquire`/`create` | `acquire`/`create` | durable id, wrong-id, lock tests |
| `runOperation` | `run_operation` | prompt/steer/abort/model/thinking tests |
| `attach`/`requireAttached`/`forConnection` | `attach`/`require_attached`/`for_connection` | attachment-state and unattached-control tests |
| `maybeDispose`/`terminate` | `maybe_dispose`/`terminate` | disconnect/terminal-error tests |
| `normalizedSnapshot`/`broadcastSnapshot` | same names | progress/snapshot broadcast tests |
| `toMetadata` | `metadata_from_snapshot` | list tests |

## Branches
- Server-assigned ID mismatch → dispose + `invalid_request` (`rejects_and_disposes_a_service_runtime_with_the_wrong_server_assigned_id`).
- Busy prompt rejects instead of queueing; steer/abort work while a prompt response is pending (`does_not_queue_prompts_and_processes_steer_and_abort_while_a_prompt_is_pending`).
- Detach is idempotent; unrelated attachments stay live (`creates_server_assigned_durable_ids...`, `keeps_multiple_attachments...`).
- Runtime ID change → `invalid_request`; runtime terminal error closes attached connections (`disconnects_attached_clients...`).
- Stored metadata order is preserved by `TestServerService` (insertion-order vector) and live-only metadata is appended; merged metadata keeps backend `parentSessionId` while refreshing live fields (`preserves_backend_metadata...`).

## Gaps / deviations
- TS stores an `openingSessions` promise map for per-ID concurrent opens; Rust serializes acquisition through one `acquire_lock`. Observable behavior (single live runtime, waiting for disposal, lock errors) is covered; different IDs acquire sequentially rather than concurrently.
- Live-session ordering for metadata not present in the service follows insertion order in Rust (TS `Map` order); the earlier `BTreeMap` ordering was replaced after differential testing.
