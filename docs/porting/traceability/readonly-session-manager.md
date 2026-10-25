# Readonly session manager

Common-pin source: `coding-agent/src/core/readonly-session-manager.ts`45lines. Native storage-layer module `readonly_session_manager.rs`, forwarded by existing core exports. Footer prerequisite approved2026-10-24.

| Export/branch | Native representation | Proof |
|---|---|---|
| createReadonlySessionManager21methods | Private Arc/Mutex session; scalar/optional getters cloned; object/list/tree getters owned | Missing API compile RED→owned snapshot/live/goal test GREEN at8dd3b1de; actual-source differential now added |
| readIsolatedTree | Underlying native get_tree already creates owned nodes/entries; no extra JS structuredClone pass | Returned tree entry mutation cannot reach stored state, test pending |
| setSessionGoalJson/clearSessionGoalJson | Only explicit mutators forwarded; original Result errors retained | In-memory goal forwarding fixture; disk errors inherited, bounded proof pending |

No aliasing deviation: frozen already promises isolated snapshots. No public backing handle or structural session mutator. Footer override formerly exposed the mutable SessionManager mutex; typed override test is compile RED against that public boundary. It now holds only ReadonlySessionManager, with real-footer/default-footer/interactive construction sites mechanically adapted. Source/runtime and consumer gates remain pending; not a done row yet.
