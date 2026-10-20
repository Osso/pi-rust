# server/sessions.ts traceability

Source: `packages/server/src/sessions.ts`. Rust: `crates/pi-server/src/sessions.rs`.

| TS item | Rust | Proof |
|---|---|---|
| `LiveSessionManager` | `LiveSessionManager` | `tests/sessions.rs` (15 tests) |
| `executeCommand` list/create/attach/detach/prompt/steer/abort/set_model/set_thinking | `execute` + command dispatch | `tests/sessions.rs`, `tests/conformance.rs` |
| `disconnect` | `disconnect` | busy-work/terminal-error tests |
| `listMetadata` | `list_metadata_inner` | metadata merge/restart tests |
| `close` (incl. `Promise.allSettled(openingSessions)`) | `close` + `settle_openings` | graceful close conformance test |
| `acquire`/`create` + `openingSessions` map | `acquire`/`lookup`/`lead_opening`/`create`; `openings` map of `watch` receivers | durable id, wrong-id, lock tests; `attaches_an_unrelated_live_session_while_another_session_is_still_opening`, `concurrent_opens_of_one_session_share_a_single_service_open`, `concurrent_opens_share_the_failure_of_the_single_service_open` |
| `runOperation` | `run_operation` | prompt/steer/abort/model/thinking tests |
| `attach`/`requireAttached`/`forConnection` | `attach`/`require_attached`/`for_connection` | attachment-state and unattached-control tests |
| `maybeDispose`/`terminate` | `maybe_dispose`/`terminate` | disconnect/terminal-error tests |
| `normalizedSnapshot`/`broadcastSnapshot` | `normalized_snapshot`/`broadcast_snapshot`/`broadcast_snapshot_now`/`broadcast_loop` | progress/snapshot broadcast tests; strict stream differential |
| `toMetadata` | `metadata_from_snapshot` | list tests |

## Branches
- Server-assigned ID mismatch → dispose + `invalid_request` (`rejects_and_disposes_a_service_runtime_with_the_wrong_server_assigned_id`).
- Busy prompt rejects instead of queueing; steer/abort work while a prompt response is pending (`does_not_queue_prompts_and_processes_steer_and_abort_while_a_prompt_is_pending`).
- Detach is idempotent; unrelated attachments stay live (`creates_server_assigned_durable_ids...`, `keeps_multiple_attachments...`).
- Runtime ID change → `invalid_request`; runtime terminal error closes attached connections (`disconnects_attached_clients...`).
- Stored metadata order is preserved by `TestServerService` (insertion-order vector) and live-only metadata is appended; merged metadata keeps backend `parentSessionId` while refreshing live fields (`preserves_backend_metadata...`).
- Snapshot broadcasts are queued per live session in call order (`broadcast_loop`), reproducing the TS microtask order in which a runtime-event snapshot is enqueued before the operation's own snapshot and response. `tests/differential_stream.rs` compares the resulting raw frame order.

## Gaps / deviations
- Per-id `openingSessions` is mirrored: the first caller leads the service open, later callers for the same id join it and receive the same result; different ids open concurrently. A failed open reaches joiners as `SharedServiceError` (Rust errors are not `Clone`), which `to_protocol_error` unwraps via `unwrap_shared_error` so the protocol code is the original's. If a leader is cancelled before settling, joiners retry with their own open future (TS has no cancellation).
- Live-session ordering for metadata not present in the service follows insertion order in Rust (TS `Map` order); the earlier `BTreeMap` ordering was replaced after differential testing.
