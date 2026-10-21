# Frozen OAuth registry and result provenance

Source `packages/ai/src/utils/oauth/index.ts`, `types.ts`, public `oauth.ts` at `4f9e6f8a4`.
Rust `crates/pi-ai/src/oauth.rs`, `oauth_registry.rs`.

| Source surface | Native mapping | Proof / status |
|---|---|---|
| Provider interface/id/info/callbacks/credentials | `OAuthProviderInterface`, aliases, existing callback trait | fake-provider public tests; typed Rust excludes invalid JS values |
| Builtin insertion order | five frozen IDs, Codex GC/Team distinct identity, frozen names | tracked registry test proves IDs/order; external synthetic builtin probe proves names/mappings, `/tmp/pi-rust-repin-auth/VERIFY-PARTIAL.md` |
| get/register/unregister/reset/getAll/info | same-ID replacement retains slot; unregister restores builtin or removes custom | registry test; extended restore/order mutation proof pending |
| refreshOAuthToken | lookup, unknown-provider failure, direct provider result | registry test |
| getOAuthApiKey | lookup first; missing credentials returns None; refresh only expired; wrap refresh errors; derive key then return credentials | registry fake-provider test only; coding-agent public refresh/rotation remains segregated, not integrated |
| provider getApiKey | access string | external synthetic builtin mapping probe; no live provider/refresh proof |
| Copilot modifyModels | preserve other providers, filter account model IDs when present, derive URL from token/domain | external synthetic 20-combination builtin probe; not login/refresh protocol proof |
| built-in login/refresh flows | existing native OAuthAuth flow bridge | NOT revalidated at common pin; upstream provider-module slices own protocol/callback deltas |
| direct provider helper exports (loginAnthropic etc.) | existing old modules, not frozen public exports | pending provider-module port; not claimed complete |

No real providers, credentials, login, or network used. Builtin flow bridge is compilation/integration scaffolding only, not current-pin provider-flow parity proof.
