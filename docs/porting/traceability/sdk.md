# sdk.ts traceability

Rust: `crates/pi-coding-agent/src/core/sdk.rs`.

| TS | Rust | Tests |
| --- | --- | --- |
| `createAgentSession` | `create_agent_session` | `tests/sdk.rs` (4), `tests/differential_agent_session.rs` |
| `createAgentSessionWithInternalOptions` | same | — |
| `CreateAgentSessionOptions` | same | — |
| `CreateAgentSessionResult` | same | — |
| `rulesScopeForRuntimeRole` | gap (`DefaultResourceLoader`) | — |
| Re-exported tool factories / `withFileMutationQueue` | not re-exported here; callers use `core::tools` | — |

## Gaps

- `DefaultResourceLoader` and `createAllToolDefinitions` are not created by `sdk`; `resource_loader` is required and `base_tools` is the injected tool seam. Recorded deviation until `ca-cli` ports the loader.
- Extension runner ref for `before_provider_headers` / `before_provider_request` / `after_provider_response` is not wired; `transform_context` is not wired.
- `restorePersistedSessionSettings` (control-db metadata) is skipped.
- `createAgentSession` returns `Err(formatNoModelsAvailableMessage())` when no model exists; TS returns a session with `model: undefined` (typed-core exclusion).
- Block-images conversion is implemented; tool-result image normalization is not.
