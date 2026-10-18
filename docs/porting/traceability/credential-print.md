# Coding-agent credential print traceability

Pinned source: `packages/coding-agent/src/cli/credential-print.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/cli/credential_print.rs`.

Process/credential module; behavioral proof at the auth-storage/model-runtime boundary (`tests/credential_print.rs`, 8 tests, ports of `test/credential-print.test.ts`).

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `credential-print.ts:7` | Default bearer-token minimum validity is 30 minutes | `DEFAULT_BEARER_TOKEN_MIN_EXPIRY_MS`; bearer print path in `prints_bearer_tokens_resolved_from_an_authorization_header` |
| `credential-print.ts:20-29` | Validate arguments, list credential types, reject unknown providers with the source message | `reports_unknown_providers`, `reports_missing_targets_and_unknown_options` |
| `credential-print.ts:30-46` | Explicit provider + model resolves through `resolveCliModel` and reports resolution errors | `resolves_a_model_for_an_explicit_provider` |
| `credential-print.ts:47-58` | Without `--provider`, only credentialed providers matching the model are considered, skipping `Using custom model id` warnings; no match reports the source message | `reports_unmatched_models_without_a_provider`, `reports_multiple_matching_providers` |
| `credential-print.ts:60-77` | Kind/credential-type filtering, per-kind OAuth validity floor, header/API-key extraction | `prints_a_resolved_api_key`, `prints_bearer_tokens_resolved_from_an_authorization_header` |
| `credential-print.ts:79-90` | Single match prints; none reports the OAuth/type-specific or generic messages; multiple reports the joined provider list | `reports_provider_credential_type_mismatches`, `reports_multiple_matching_providers`, `reports_unmatched_models_without_a_provider` |

## Semantics

- `CredentialPrintKind` mirrors the source `Exclude<AuthCommandKind, "check">`; `check` is not representable.
- `CredentialPrintError` distinguishes source `AuthCommandError` (`is_command_error`) from runtime failures, because `main.ts` prints only the former's message.
- Runtime `getAuth`/`listCredentials` rejections become `CredentialPrintError::Runtime` with the same message text.

## Gaps

- Expired-OAuth refresh is exercised by the model-runtime slice; this module only forwards the validity floor and signal.
- The TypeScript test's `vi.fn` refresh interception has no Rust equivalent.

## Discrepancies

None found.
