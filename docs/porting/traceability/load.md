# auth/oauth/load traceability

Pinned source: `packages/ai/src/auth/oauth/load.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `OAuthFlowLoaders` | Seven flow constructors; Radius takes `{name, gateway}`. | `OAuthFlowLoaders`; `loader_registry_defaults_overrides_and_bun_registration`. |
| `registerBundledOAuthFlowLoaders` | Replace the registered loader set. | `register_bundled_oauth_flow_loaders` (process-global `RwLock`). |
| `loadAnthropicOAuth` | Bundled loader, else module import. | `load_anthropic_oauth`: registered loader, else `AnthropicOAuth::new`. |
| `loadOpenAICodexOAuth` | Same. | `load_openai_codex_oauth`. |
| `loadGitHubCopilotOAuth` | Same. | `load_github_copilot_oauth`; fallback is an explicit unavailable-flow gap because `github-copilot.ts` is outside this slice. |
| `loadOpenRouterOAuth` | Same. | `load_openrouter_oauth`. |
| `loadKimiCodingOAuth` | Same. | `load_kimi_coding_oauth`. |
| `loadXaiOAuth` | Same. | `load_xai_oauth`. |
| `loadRadiusOAuth` | Loader with options, else `createRadiusOAuth`. | `load_radius_oauth`. |

## Exclusions and discrepancies

- TypeScript dynamically imports modules so browser builds can exclude Node-only flows; Rust links the ported flows statically and constructs them directly when no bundled loader is registered. Observable flow behavior is unchanged.
- GitHub Copilot OAuth is not ported in this slice; the default loader returns an `UnavailableOAuth` whose login/refresh/toAuth fail with `GitHub Copilot OAuth is not ported yet`. Registered bundled loaders can still supply it.
- The loader functions are synchronous in Rust; the source returns promises. Callers in the ported Rust surface do not await module loading.
