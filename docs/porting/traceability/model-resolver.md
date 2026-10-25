# Model resolver

Frozen: `packages/coding-agent/src/core/model-resolver.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`; all705 lines audited against `pi-coding-agent-models/src/core/model_resolver.rs`.

| Exports / branches | Proof |
| --- | --- |
| `defaultModelPerProvider`: ordered36-entry table | Actual-source defaults comparison, no old-only providers |
| `findExactModelReferenceMatch`: trim/casefold, canonical then split provider/id then bare id; ambiguity rejected | Actual-source exact matrix |
| `parseModelPattern`, isAlias, tryMatchModel: exact/fuzzy human name/id, alias/date locale order, recursive colon suffix, strict rejection/warning precedence | Actual-source parse matrix plus native concrete fixtures |
| `resolveModelScopeWithDiagnostics`, `resolveModelScope`: glob detection/minimatch, optional valid suffix, id/canonical matching, duplicate suppression, warning records/printing | Actual-source scope matrix; native helpers and shared default-Chalk stdout styling for console.warn |
| `resolveCliModel`, buildFallbackModel: all-model registry, provider last-wins casefold map, provider inference/raw-id/auth alternatives, suffix/default fallback and reasoning override | Actual-source CLI matrix, empty/blank model/provider and provider-collision cases; native full-registry auth/default control |
| `findInitialModel`: truthy CLI pair, scoped unless continuing, configured saved default, ordered provider defaults then first available/none | Actual-source initial matrix; added RED for empty CLI provider wrongly overriding scoped selection; native priority/default fixtures |
| `restoreModelFromSession`: restored+auth; missing/unauth warning; current fallback; preferred/first available; no available; print toggle | Actual-source full-model/fallback/stdout/stderr comparison across6 scenarios ×2 print settings ×2 color modes in private native processes |

`ModelResolverRegistry` borrows the four frozen-used methods; ModelRegistry is its sole production implementation. Parent accepted this structural test seam on2026-10-23. The oracle uses real frozen in-memory ModelRegistry/AuthStorage, fills its exposed backing model array, and overrides only getAvailable to select deterministic fixtures. No live provider calls.

RED `/tmp/settings-models-resolver-red.log`: restore success went to stderr instead of stdout; empty CLI provider selected gpt-4o instead of the scoped gpt-5.5. Native restore now uses stdout/dim, warnings stderr/yellow, with the default Chalk stdout color authority as frozen does. Source full-model values and fallback text are not stripped in the new restore proof. Console fixtures are standalone because they self-spawn.

`findInitialModel` returns a native Error instead of directly terminating the embedding process; the CLI owns error reporting/termination. Owned model returns and opaque scalar provider/id boundaries retain the common approvals. Pathological input/runtime/stack-format oddities are outside the parity bar; no new carrier or fallback behavior is introduced.

Focused GREEN and warning-mode clippy are recorded after the implementation commit in `/tmp/settings-models-proof-ledger.md`.
