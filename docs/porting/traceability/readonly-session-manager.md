# Readonly session manager

Common-pin `coding-agent/src/core/readonly-session-manager.ts`45lines. [Contract](../../specs/readonly-session-manager.md). Native storage-layer `readonly_session_manager.rs` is forwarded through existing core exports. Whole prerequisite approved2026-10-24.

| Frozen export/branch | Native behavior | Proof |
|---|---|---|
| createReadonlySessionManager | Private live Arc/Mutex session; no exposed backing handle or structural mutators | Missing API and unsafe footer-field compile RED→GREEN; live post-construction append and footer override read |
| getCwd/getSessionDir/getSessionId/getSessionFile/getLeafId/getLabel/getSessionName/hasSessionNameState/isSubagentSession/getSubagentName | Current cloned scalar/optional values | Real frozen SessionManager wrapper vs native values, readonly target first case |
| getLeafEntry/getEntry/getBranch/buildContextEntries/getHeader/getEntries | Isolated owned read values, including nested arbitrary JSON | Source mutates every object result; native snapshot/header/tree mutation cannot affect stored entry; native values compared with source |
| readIsolatedTree/getTree | Existing native get_tree already creates owned nodes/entries; no redundant structuredClone pass | Root-isolation proof GREEN; added128-descendant mutation case pending final run |
| getSessionGoalJson/getSessionGoalJsonForSession/setSessionGoalJson/clearSessionGoalJson | Only explicit mutators forwarded; live state and original Result errors | Source/native in-memory goal read/write/clear GREEN; added invalid-directory database failure compares complete native error values, pending final run |

Footer override now stores this view instead of a mutable SessionManager mutex. Core/default/interactive footer adapters read snapshots; they do not regain a backing handle. The old type-erased/native mutable override gap is retired by this runtime change, not by a documentation waiver.

## Approved typed input boundary

| Gap | Classification/evidence |
|---|---|
| Arbitrary structural ReadonlySessionManager implementations | Parent2026-10-24 approves a typed-context exclusion: native factory accepts concrete Arc<Mutex<SessionManager>>, not a21-method custom backend. Every built-in frozen footer caller supplies a real SessionManager/its readonly projection; no built-in consumer passes a custom structural manager. No trait/backend extensibility is added |

Frozen already promises mutable isolated snapshots; this module does not need an aliasing deviation. Broader native owned-reference conventions remain in AGENTS. Final consumer/static/coverage gates are pending; current proof ledger is `/tmp/pi-rust-repin-lossless-interactive/footer-provider-proof-ledger.md`.
