# Coding-agent auth check traceability

Pinned source: `packages/coding-agent/src/cli/auth-check.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/cli/auth_check.rs`.

This is a process/credential module: it reads `auth.json` through `core::auth_storage` and resolves auth through `core::model_runtime`. Proof is behavioral at that boundary (`tests/auth_check.rs`, 9 tests, ports of `test/auth-check.test.ts`), not an oracle differential.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `auth-check.ts:8-13` | `ready`/`not_ready`/`invalid` and the four reason literals | `AuthCheckStatus::as_str`, `AuthCheckReason::as_str`; result assertions in every test |
| `auth-check.ts:23-35` | Validate arguments, resolve `--model` through `resolveCliModel`, throw `AuthCommandError` on resolution failure or missing provider | `resolves_the_provider_from_the_model_flag` |
| `auth-check.ts:36-38` | Model-load error yields `invalid`/`invalid_state` | Covered through `reports_malformed_auth_state_as_invalid` and the runtime error surface; a models.json error needs a malformed models file, exercised by the model-runtime slice |
| `auth-check.ts:39-41` | Unknown provider yields `not_ready`/`provider_not_found` | `reports_an_unknown_provider_as_not_ready` |
| `auth-check.ts:42-49` | Missing credential yields `credentials_not_configured`; `refresh` re-checks resolved auth; success reports the auth type | `reports_a_configured_provider_as_ready`, `does_not_treat_an_unresolved_stored_environment_reference_as_configured` |
| `auth-check.ts:50-52` | Thrown auth errors map to `invalid`/`invalid_state` | `reports_malformed_auth_state_as_invalid` |
| `auth-check.ts:55-64` | `getProviderCredential` returns stored OAuth access without refresh, otherwise the resolved credential | `reads_api_key_credentials_without_refresh`, `reads_stored_oauth_access_tokens_without_refreshing` |
| `auth-check.ts:66-73` | `createAuthCheckModelRuntime` uses an in-memory models store, no network, no create refresh | `creates_an_auth_check_runtime_with_builtin_providers` |
| `auth-check.ts` (no file creation) | Read-only storage never creates `auth.json` or its directory | `does_not_create_an_auth_file_or_its_parent_directory` |

## Dependency fix

`ReadOnlyAuthStorage.read` in the merged auth-storage slice returned stored keys unresolved. The pinned source resolves template references and leaves command references literal. Fixed in `core::auth_storage` (`resolved_read_only_credential`) and covered by `tests/auth_storage.rs::read_only_storage_resolves_templates_without_executing_commands`; the previous test asserted the incorrect behavior and was corrected.

## Gaps

- OAuth refresh interception (TS replaces `provider.auth.oauth.refresh` with a spy) has no Rust equivalent; refresh behavior itself is owned and tested by the model-runtime slice.
- `AuthCheckReason::CredentialNotAvailable` is produced by the CLI consumer (`main.ts`), not this module.

## Discrepancies

None observed beyond the dependency fix above.

Q18 native CLI auth prompt/credential text stays `JsString` through IO callbacks; browser URL encodes at spawn, not presentation. Behavioral proof status: [lossless auth](lossless-auth.md); coding-agent tests unexecuted until library compiles.
