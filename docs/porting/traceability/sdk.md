# sdk.ts traceability

Rust: `crates/pi-coding-agent/src/core/sdk.rs`.

| TS | Rust | Tests |
| --- | --- | --- |
| `createAgentSession` | `create_agent_session` | `tests/sdk.rs` (4), `tests/differential_agent_session.rs` |
| `createAgentSessionWithInternalOptions` | same | — |
| `CreateAgentSessionOptions` | same | — |
| `CreateAgentSessionResult` | same (`extensions_result: LoadExtensionsResult`) | export/runtime tests |
| `rulesScopeForRuntimeRole` | `rules_scope_for_runtime_role` | DefaultResourceLoader tests |
| `restorePersistedSessionSettings` | inline restore of sandbox profile | `create_agent_session_*` |
| Re-exported tool factories / `withFileMutationQueue` | not re-exported here; callers use `core::tools` | — |

## Behavior

- Model resolution: explicit model, persisted session settings, session context, then
  `find_initial_model`. A missing model yields `modelFallbackMessage` and an `unknown`
  default model with `thinkingLevel: "off"`; prompting then fails on authentication,
  matching TS (`create_agent_session_keeps_running_without_models`).
- `DefaultResourceLoader` is created and reloaded when `resourceLoader` is omitted;
  `extensionFactories` and `rulesScope` are forwarded.
- Persisted sandbox profile, model, and thinking level are restored from control-db
  metadata before model selection; control-db read failures propagate instead of
  being dropped.
- `streamFn` wires settings timeouts, provider retry, attribution headers, and the
  provider-retry notifier; `convertToLlm` honors `images.blockImages`.
- `extensions_result` comes from the resource loader.

## Gaps

- `base_tools` remains the injected tool override seam; when empty, `AgentSession`
  builds `createAllToolDefinitions` from current settings.
- Extension runner ref for `before_provider_headers` / `before_provider_request` /
  `after_provider_response` and `transformContext` are not wired.
- `multiAgentStore`, `onSharedChannelMessageDelivered`, and `sessionStartEvent` are
  forwarded to `AgentSession`; mutation-target resolver binding remains in `main`
  (gap).
