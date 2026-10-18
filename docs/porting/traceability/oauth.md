# oauth / bun-oauth / extension-oauth-types traceability

Pinned sources at `532f7d12cd19235081bd4d6e425bde185652d86f`:

- `packages/ai/src/oauth.ts`
- `packages/ai/src/bun-oauth.ts`
- `packages/ai/src/compat/extension-oauth-types.ts`

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `oauth.ts` | Type-only barrel: `OAuthAuthInfo`, `OAuthCredentials`, `OAuthDeviceCodeInfo`, `OAuthLoginCallbacks`, `OAuthPrompt`, `OAuthSelectOption`, `OAuthSelectPrompt`. No runtime values. | `crate::oauth` re-exports the compat types; no runtime behavior to test. |
| `extension-oauth-types.ts` | Legacy callback interfaces, optional progress/manual-code/select/signal members, credential alias. | `compat::extension_oauth_types` structs and `OAuthLoginCallbacks` trait with default optional methods; `OAuthCredentials` aliases `auth::OAuthCredential`. |
| `bun-oauth.ts` | `registerBunOAuthFlows` registers all bundled flows. | `bun_oauth::register_bun_oauth_flows`; `loader_registry_defaults_overrides_and_bun_registration`. |

## Exclusions and discrepancies

- `oauth.ts` is type-only in TypeScript and therefore has no differential runtime surface.
- `registerBunOAuthFlows` registers a GitHub Copilot placeholder because that flow is not ported in this slice.
- The extension callback interface is typed; JavaScript callers may pass partial objects with optional members. Rust callers implement the trait, with optional members expressed as default methods.
