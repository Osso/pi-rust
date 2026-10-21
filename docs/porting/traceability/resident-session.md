# resident-session.ts — whole-module implementation, proof pending

Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, `packages/coding-agent/src/core/resident-session.ts` (whole file read).

| Symbol / source branch | Native / targeted proof |
|---|---|
| `ResidentRole` | `ResidentRole::{Architect,Supervisor}` |
| `isResidentSession` exact IDs and slash/backslash complete segments | Existing predicate unchanged; prior `repin_session_storage` listing matrix retained |
| `prepareResidentSessionFile` recursive mkdir, regular-file filter, UTF-16 latest / canonical | `prepare_resident_session_file`; `repin_resident_session::source_created_trees_native_prepare_and_source_reader_reciprocal` |
| Live basename insertion, live/latest/canonical precedence | Paired actual source/native own-PID fixtures; missing file, unmatched file, directory basename |
| `findLiveResidentSessionPath`: null-agent role, missing listener/path, dead/non-Pi, resolved dirname, exact extension | Paired own-PID matrix; owned disposable source children for dead/non-Pi; relative lexical paths and symlink alias rejection |
| Metadata removal before unlink; immediate FS/DB failures | Actual-source mkdir/ancestor/scandir/unlink failure fixtures; invalid SQLite fixture |
| `isResidentSessionForRole`: role ID / path segment, missing path, exact live skip | Reciprocal metadata fixtures with other role, slash/backslash, prefix/suffix, symlink existence |

Dedicated oracle: `oracle/resident-session-oracle.ts`; tests: `crates/pi-coding-agent/tests/repin_resident_session.rs`. [Contract](../../specs/resident-session.md).

RED is an E0432 missing-API compiler failure: **no runtime assertions reached**. Source-only error probes confirmed EEXIST/mkdir, ENOTDIR/mkdir, EACCES/scandir and EACCES/unlink under the mandatory guard before implementation. Full GREEN and mutation evidence pending. No whole-module promotion yet.

Native and TS self-PID branches intentionally use different actual processes; no fixture claims to impersonate a full Pi runtime. Disposable dead/non-Pi children are owned; no foreign PID signaling, services or live-state cleanup. No consumer wiring required by this source module.

Local POSIX Node lexical join/resolve preserve leading relative `..`: shared `utils::paths::node_join` drops those components and `node_resolve` does not anchor relative paths to cwd. Shared NodeFs error record formatting is reused. Shared files remain untouched.
