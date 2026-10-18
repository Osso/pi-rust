# auth-guidance traceability

Pinned source: `packages/coding-agent/src/core/auth-guidance.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/core/auth_guidance.rs`.

Differential proof: `oracle/auth-guidance-oracle.ts` with `PI_PACKAGE_DIR` set; the Rust payload runs
in a child process with the same variable (`tests/differential_config_auth_state.rs`). Focused test:
`tests/trust_manager.rs::login_guidance_points_at_the_packaged_docs`.

| TypeScript | Rust | Proof |
|---|---|---|
| `getProviderLoginHelp` | `get_provider_login_help` | focused + differential |
| `formatNoModelsAvailableMessage` | `format_no_models_available_message` | focused + differential |
| `formatNoModelSelectedMessage` | `format_no_model_selected_message` | focused + differential |
| `formatNoApiKeyFoundMessage` (`unknown` → "the selected model") | `format_no_api_key_found_message` | focused (three providers) + differential |

## Gaps

- None.

## Discrepancies

- None observed.

## Proof inventory

- Focused cases: 1 (four messages).
- Differential: 1 request covering all four messages for three providers.
