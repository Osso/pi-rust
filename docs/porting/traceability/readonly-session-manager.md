# Readonly session manager

Common-pin source: `coding-agent/src/core/readonly-session-manager.ts`45lines. Native storage-layer module `readonly_session_manager.rs`, forwarded by existing core exports. Footer prerequisite approved2026-10-24.

| Export/branch | Native representation | Proof |
|---|---|---|
| createReadonlySessionManager21methods | Private Arc/Mutex session; scalar/optional getters cloned; object/list/tree getters owned | Missing API compile RED; snapshot/live/goal test GREEN pending |
| readIsolatedTree | Underlying native get_tree already creates owned nodes/entries; no extra JS structuredClone pass | Returned tree entry mutation cannot reach stored state, test pending |
| setSessionGoalJson/clearSessionGoalJson | Only explicit mutators forwarded; original Result errors retained | In-memory goal forwarding fixture; disk errors inherited, bounded proof pending |

No aliasing deviation: frozen already promises isolated snapshots. No public backing handle or structural session mutator. Source/runtime differential and footer consumers remain in progress; not a done row yet.
