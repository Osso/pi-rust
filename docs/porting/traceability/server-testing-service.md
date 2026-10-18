# server/testing/service.ts traceability

Source: `packages/server/src/testing/service.ts`. Rust: `crates/pi-server/src/testing/service.rs`.

| TS item | Rust | Proof |
|---|---|---|
| `TEST_MODEL` | `TEST_MODEL` (`LazyLock<ModelMetadata>`) | conformance/session tests |
| `Deferred` | `Deferred<T>` (`watch` channel) | delayed list, disposal, listener tests |
| `TestSessionRuntime` | `TestSessionRuntime` | prompt/steer/abort/model/thinking/dispose tests |
| `TestServerService` | `TestServerService` (cloneable shared state) | all integration tests |
| `seed` | `seed`/`seed_named`/`seed_with` | session tests |
| `delayNextList` | `delay_next_list` | handshake/catch-up/out-of-order tests |
| `latestRuntime` | `latest_runtime` | session/conformance assertions |

## Fidelity notes
- Stored snapshots are shared through `Arc<StoredSession>` so runtime updates persist for lazy reopen/restart tests, matching the TS shared object.
- Stored session ordering is insertion order (vector), matching `Map`.
- `disposed` is a single-resolution async value; `dispose_count` is an atomic counter.
- `finish_prompt_transcript` builds assistant items with `AssistantTranscriptItem::new_runtime`, preserving the TS literal's `status`-before-`model` insertion order on the wire. `tests/differential_stream.rs` compares the raw frames produced by this path against the pinned runtime.
- `tests/differential_stream.rs` also emits tool running/complete/error and assistant streaming/error/aborted progress items. The pinned packages contain no runtime producer for those objects, so both sides construct them in schema declaration order.
