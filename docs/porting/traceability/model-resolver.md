# Model resolver — Q15 re-port in progress

Frozen: `packages/coding-agent/src/core/model-resolver.ts` at [the common pin](../source-lock.json).
Rust: `crates/pi-coding-agent/src/core/model_resolver.rs`.

| Frozen exports/branches | Current proof |
|---|---|
| defaultModelPerProvider | Actual frozen RED → GREEN: exact frozen 36-entry record, including Team and absence of six old-only providers. |
| findExactModelReferenceMatch, parseModelPattern | Actual frozen exact/parse matrices GREEN; proof scope is the deterministic fixture lists, not arbitrary malformed runtime inputs. |
| resolveModelScopeWithDiagnostics/resolveModelScope | Actual frozen RED → GREEN: warning diagnostic shape and direct glob matching (including bracket classes); no old-only exact-ID shortcut. |
| resolveCliModel | First-exact/catalog/auth/provider/fuzzy matrix GREEN. New actual frozen REDs: empty model/provider use JS falsiness, and case-folded provider collisions keep the last value. Corrected using native JS lowercasing and an insertion-ordered map; edge GREEN pending. |
| findInitialModel, restoreModelFromSession | Initial matrix GREEN. Restore Rust behavior tests GREEN; a frozen restore differential is still pending. No old runtime snapshots remain in production. |

## Rust implementation mechanism

`ModelResolverRegistry` borrows exactly four frozen-used methods: get_all, get_available,
find and has_configured_auth(&Model). It has no state, defaults or fallback; ModelRegistry
is its sole production implementation. Parent accepted this structural test seam on 2026-10-23.

`oracle/model-resolver-oracle.ts` uses real ModelRegistry.inMemory and AuthStorage.inMemory.
Fixture setup fills the backing model array through frozen getAll; only getAvailable is overridden
to provide the exact deterministic available-model list. find and configured-auth checks stay frozen.
Rust fixture lists mirror those inputs. The full_registry_resolves_authenticated_default_and_unconfigured_explicit_cli_model
control uses an unmodified full ModelRegistry and is GREEN. Six primary differential categories plus four CLI boundary tests expose REDs independently. Rust behavior target: 41 GREEN at c4c3ca99; later CLI-boundary changes require focused revalidation.

No whole-module parity claim. Logs and revision-specific proof remain in the Q15 artifact ledger.
