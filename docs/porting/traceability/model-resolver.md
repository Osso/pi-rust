# Model resolver — Q15 re-port in progress

Frozen: `packages/coding-agent/src/core/model-resolver.ts` at [the common pin](../source-lock.json).
Rust: `crates/pi-coding-agent/src/core/model_resolver.rs`.

| Frozen exports/branches | Current proof |
|---|---|
| defaultModelPerProvider | Revalidation pending; old-only provider entries still need removal. |
| findExactModelReferenceMatch, parseModelPattern | Actual frozen oracle now imports the pinned module; matrix acceptance pending. |
| resolveModelScopeWithDiagnostics/resolveModelScope | Actual frozen RED: diagnostics have `type:"warning"`, message, pattern, not old-only code. Production carrier corrected; remaining scope branches pending. |
| resolveCliModel | Exact/qualified/ambiguous/fuzzy/unknown selection matrix pending. |
| findInitialModel, restoreModelFromSession | Saved/default/scoped/auth/available branches pending; no old runtime snapshots remain in production. |

## Rust implementation mechanism

`ModelResolverRegistry` borrows exactly four frozen-used methods: get_all, get_available,
find and has_configured_auth(&Model). It has no state, defaults or fallback; ModelRegistry
is its sole production implementation. Parent accepted this structural test seam on 2026-10-23.

`oracle/model-resolver-oracle.ts` uses real ModelRegistry.inMemory and AuthStorage.inMemory.
Fixture setup fills the backing model array through frozen getAll; only getAvailable is overridden
to provide the exact deterministic available-model list. find and configured-auth checks stay frozen.
Rust fixture lists mirror those inputs. The full_registry_resolves_authenticated_default_and_unconfigured_explicit_cli_model
control uses an unmodified full ModelRegistry; execution is pending.

No whole-module parity claim. Logs and revision-specific proof remain in the Q15 artifact ledger.
