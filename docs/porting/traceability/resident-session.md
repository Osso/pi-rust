# resident-session.ts — predicate only

Frozen `4f9e6f8a4`, `core/resident-session.ts:8-14`.

| Export / branch | Rust / proof |
|---|---|
| `isResidentSession` IDs supervisor/architect | `resident_session::is_resident_session`, ordinary listing actual-source differential |
| Complete supervisor-sessions/architect-sessions segment, slash or backslash | Same predicate, nine-case filesystem + cached-metadata matrix in `repin_session_storage::resident_sessions_are_excluded_from_ordinary_filesystem_listing` |
| Ordinary IDs, case-sensitive names, prefix/suffix lookalikes | Same matrix preserves ordinary sessions |
| Preparation, live-listener lookup, stale file/metadata cleanup | Pending lifecycle owner; no whole-module promotion |

Source-backed RED at12b382a9 before production; focused GREEN4fdf1250. Manager excludes resident sessions in metadata results and filesystem discovery, including no-control-DB calls. No lifecycle behavior added.
