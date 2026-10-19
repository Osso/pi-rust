# sdk.ts traceability

Rust: `crates/pi-coding-agent/src/core/sdk.rs`.

| TS | Rust | Tests |
| --- | --- | --- |
| `createAgentSession` | `create_agent_session` | `tests/sdk.rs` (4), `tests/differential_agent_session.rs` |
| `createAgentSessionWithInternalOptions` | same | — |
| `CreateAgentSessionOptions` | same, plus Rust-native `base_tool_definitions`, `custom_tool_definitions`, `multi_agent_store`, `disable_runtime_coordination_inbound` carriers | `create_agent_session_forwards_observer_coordination_options` |
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
- The extension runner ref is created by the SDK and populated by `AgentSession`.
  The same ref backs `transformHeaders` (`before_provider_headers`),
  `onPayload` (`before_provider_request`), `onResponse`
  (`after_provider_response`), and `transformContext` (`context`), matching the
  source closure reads. Test: `create_agent_session_wires_extension_provider_hooks`
  (headers/response/context handlers, patched context reaches the provider).
- `multiAgentAgentId`, `multiAgentParentSessionId`, `multiAgentRequiresAgentId`, and
  `supervisorDecisionRequester` are forwarded to `AgentSessionConfig`. Proof:
  `create_agent_session_forwards_multi_agent_identity_to_extensions` (tests/sdk.rs)
  reads the three identity carriers back through the extension context.
- `extensions_result` comes from the resource loader.

## Gaps

- `base_tools` remains the injected tool override seam; when empty, `AgentSession`
  builds `createAllToolDefinitions` from current settings.
- `multiAgentStore`, `onSharedChannelMessageDelivered`, and `sessionStartEvent` are
  forwarded to `AgentSession`; mutation-target resolver binding remains in `main`
  (gap).
