# Traceability: `packages/agent/src/search/index.ts`

Rust: `crates/pi-agent-core/src/search/index.rs`. Tests: `session_search.rs`, `differential_session.rs`.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `SessionSearchOptions` | `SessionSearchOptions` | search | `entryTypes`, `limit`, `signal` → `Option<CancellationToken>`. |
| `SessionSearchHit` | `SessionSearchHit` trait | search | `session_id()`/`entry_id()` accessors. |
| `SessionSearch` | `SessionSearch<H>` trait | search, differential | Async iteration becomes `BoxFuture<Result<Vec<H>, SearchError>>`. |
| re-exports from `scanning.ts` | `search::*` re-exports | search | |

## Gaps

- TS `search()` returns `AsyncIterable<T>`; Rust returns the complete hit vector. This is a typed-core API shape
  adaptation. Ordering, filtering, limits, abort checks, and duplicate-session detection are identical.
- `AbortSignal.reason` is not representable by `CancellationToken`; abort maps to `SearchError::Aborted` with the
  source fallback message `The operation was aborted`.
