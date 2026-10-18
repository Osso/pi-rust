# Traceability: `packages/agent/src/search/scanning.ts`

Rust: `crates/pi-agent-core/src/search/scanning.rs`. Tests: `session_search.rs`, `differential_session.rs`.

## Exports and branches

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `SessionSearchCandidate` | `SessionSearchCandidate` | search, differential | `fields` carries `{label}` only when a label exists. |
| `ScanningReadable` | `ScanningReadable<M>` trait | search | Implemented for `Session<M>` via `getMetadata`/`findEntries`/`getLabel`. |
| `ScanningReadableSource` | `ScanningReadableSource<M, O>` | search, differential | Dynamic source returns a `Vec` of readables in one future instead of an async iterable. |
| `ScanningSearchTextProjector` | `ScanningSearchTextProjector<M>` | search | |
| `ScanningReadableOptions` | `ScanningReadableOptions<M>` | search | |
| `ScanningSessionSearchHit` | `ScanningSessionSearchHit` | search, differential | camelCase serde shape. |
| `ScanningSessionSearchOptions` | `ScanningSessionSearchOptions<M, O, H>` | search, differential | `match` → `matcher`, `createHit` → `create_hit`, `sourceOptions` mapper. |
| `defaultSearchText` | `default_search_text` | search | `JSON.stringify(entry)` plus optional ` {label}`. |
| `scanReadableEntries` | `scan_readable_entries` | search, differential | Page size = query limit → options page size → 100; single-type pushdown; cursor advances to the last seq; stops on a short page. |
| `scanningEntries` | `scanning_entries` | search | |
| `arraySource` / `readablesFor` | `ScanningSource::{Readables, Dynamic}` | search, differential | |
| `defaultMatch` | `default_match` | search, differential | Lowercased substring match. |
| `throwIfAborted` | `check_aborted` | search, differential | Checked before each readable and each candidate. |
| `createDefaultScanningHit` | `default_hit` | search, differential | Custom hit types convert via `From<ScanningSessionSearchHit>`. |
| `createScanningSessionSearch` | `create_scanning_session_search` | search, differential | Normalizes `text.trim().toLowerCase()`; empty text, `limit <= 0`, and empty `entryTypes` return no hits. |
| duplicate session ids | `SearchError::DuplicateSessionId` | search | Exact `Duplicate sessionId: {id}` message. |
| `sourceOptions` | `ScanningSourceOptionsMapper` | search, differential | Receives the normalized query. |

## Branches and gaps

- Async iteration is materialized into a `Vec` per call. Pagination, label lookups, projection order, filters,
  limits, and abort checks match the source.
- Custom `THit` types must implement `SessionSearchHit` and `From<ScanningSessionSearchHit>`; TS uses structural
  typing.
- TS can throw an arbitrary `signal.reason`; Rust has no reason channel (see `search-index.md`).
