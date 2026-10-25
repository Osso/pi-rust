# Readonly session manager

Common-pin `coding-agent/src/core/readonly-session-manager.ts`45lines. [Contract](../../specs/readonly-session-manager.md). Native storage-layer `readonly_session_manager.rs` is forwarded through existing core exports. Whole prerequisite approved2026-10-24.

| Frozen export/branch | Native behavior | Proof |
|---|---|---|
| createReadonlySessionManager | Private live Arc/Mutex session; no exposed backing handle or structural mutators | Missing API and unsafe footer-field compile RED→GREEN; live post-construction append and footer override read |
| getCwd/getSessionDir/getSessionId/getSessionFile/getLeafId/getLabel/getSessionName/hasSessionNameState/isSubagentSession/getSubagentName | Current cloned scalar/optional values | Real frozen SessionManager wrapper vs native values, readonly target first case |
| getLeafEntry/getEntry/getBranch/buildContextEntries/getHeader/getEntries | Isolated owned read values, including nested arbitrary JSON | Source mutates every object result; native snapshot/header/tree mutation cannot affect stored entry; native values compared with source |
| readIsolatedTree/getTree | Existing native get_tree already creates owned nodes/entries; no redundant structuredClone pass | Root and128-descendant mutation source/native proofs GREEN; every descendant's nested data remains isolated |
| getSessionGoalJson/getSessionGoalJsonForSession/setSessionGoalJson/clearSessionGoalJson | Only explicit mutators forwarded; live state and original Result errors | Source/native in-memory goal read/write/clear GREEN; invalid-directory database failure compares complete native error values for all four operations |

Footer override now stores this view instead of a mutable SessionManager mutex. Core/default/interactive footer adapters read snapshots; they do not regain a backing handle. The old type-erased/native mutable override gap is retired by this runtime change, not by a documentation waiver.

## Approved typed input boundary

| Gap | Classification/evidence |
|---|---|
| Arbitrary structural ReadonlySessionManager implementations | Parent2026-10-24 approves a typed-context exclusion: native factory accepts concrete Arc<Mutex<SessionManager>>, not a21-method custom backend. Built-in callers are `extensions/runner.ts:369,379` (concrete SessionManager constructor input) and footer override wrapping at:199; `interactive-mode.ts:625,3555–3588` supplies its concrete childViewSessionManager (or null placeholder), never a custom structural manager. No trait/backend extensibility is added |

Frozen already promises mutable isolated snapshots; this module does not need an aliasing deviation. Broader native owned-reference conventions remain in AGENTS. Current readonly target3/3 and real-footer/default/interactive consumers pass; [checkpoint proof](../footer-data-provider-proof.md) records scoped gates. Current proof ledger is `/tmp/pi-rust-repin-lossless-interactive/footer-provider-proof-ledger.md`.
