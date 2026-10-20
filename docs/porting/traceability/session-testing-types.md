# Session testing types

Source: `packages/agent/src/harness/session/testing/types.ts` at [source-lock](../source-lock.json).
Rust: `crates/pi-agent-core/src/harness/session/testing/types.rs`.

| Source export | Native contract | Behavioral proof |
|---|---|---|
| `SessionBackendFixture` | Trait with backend-native `Repository`, borrowed `repository()`, awaited fallible `dispose()` | `session_backend_conformance`: 56 fresh-fixture runs, cleanup after backend errors and assertion failures, cleanup errors |
| `SessionBackendFixtureFactory` | Trait implemented by async closures returning `ConformanceResult<Fixture>` | Lazy construction, factory error propagation, repeatable case runs |
| `SessionBackendConformanceCase` | Named case with async `run() -> ConformanceResult<()>` | External consumer tests in core and SQLite; pinned manifest/lifecycle differential |

`SessionBackendRepository` is the adapter required because existing native repositories have inherent methods, distinct creation options, and distinct metadata types. It reuses `Session<M>`, `SessionCreateOptions`, `ForkOptions`, `SessionError`, and the existing metadata contract; it contains no storage implementation. An in-memory adapter is included. The downstream SQLite integration test implements its own adapter using existing test fixtures, with no core-to-SQLite dependency. Metadata remains backend-native, serializable/deserializable for the source's metadata copy/mutation assertions.

`ConformanceError`/`ConformanceResult` retain concrete backend/factory/disposal error types. `SuppressedConformanceError.error` is the disposal failure; `.suppressed` retains the earlier case failure, matching source `await using` ordering. Case assertion panics are caught during future polling, then returned after disposal; no task spawning or test-runner registration is required. This assertion mapping uses Rust's normal unwind mode. Callers must await cases to completion, as async cleanup cannot run after an abandoned future.

No new semantic deviation. Invalid JS payload/query exclusions are enumerated in [conformance traceability](session-testing-conformance.md).
