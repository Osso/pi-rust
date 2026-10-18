# providers/github-copilot traceability

Pinned source: `packages/ai/src/providers/github-copilot.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/providers/github_copilot.rs`.
Tests: `crates/pi-ai/tests/provider_auth.rs`,
`crates/pi-ai/tests/differential_providers.rs`.

| TypeScript | Rust | Proof |
|---|---|---|
| provider metadata (id/name/baseUrl/auth names/api map) | `github_copilot_provider` | `provider_metadata_matches_the_ts_factories`, differential provider descriptors + dispatch |
| `filterModels` non-oauth credential → unchanged | `filter_models` | `github_copilot_filters_models_to_available_model_ids` |
| `availableModelIds` not an array of strings → unchanged | `filter_models` | same |
| oauth credential with string ids → filter by set | `filter_models` | same, `github_copilot_available_models_follow_the_stored_oauth_credential` |
| `lazyOAuth({ name: "GitHub Copilot", isSubscription: true, load: loadGitHubCopilotOAuth })` | `lazy_oauth` + `load_github_copilot_oauth` | `oauth_subscription_flags_match_the_ts_factories`, `oauth_load.rs` |

## Branch notes

- `availableModelIds` is read from `OAuthCredential::extra`, where the OAuth flow
  stores it.
- An empty id array filters everything out (TS `Set` membership), matching
  `getAvailable` returning no models.

## Gaps

- None.

## Discrepancies

- None.
