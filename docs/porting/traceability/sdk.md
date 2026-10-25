# sdk.ts traceability

Rust: `crates/pi-coding-agent-core/src/core/sdk.rs`.

Q18: convert-to-LLM callback propagates native AgentError from core conversion before optional image blocking; invalid typed provider context is never omitted or Display-encoded. SDK cwd/agent_dir/control_db_path options, result fallback message and errors now use JsString. Native resolution and node_join preserve path units through auth/models path construction, SettingsManager/SessionManager/resource-loader adapters and AgentSession config; no SDK encoding boundary. `lossless_agent_session_paths` adds SDK reopen/agent-dir assertions. Execution remains blocked by scalar model-storage/settings/resource-loader dependencies and coding-library callers. `sdk-path-red.jsonl` records the blocked target before SDK migration; no runtime parity claim.

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
- Path resolution mirrors `resolvePath`: `cwd` and `agentDir` errors propagate
  instead of falling back to the raw input. Test:
  `create_agent_session_propagates_path_resolution_errors`.
- `SettingsManager::create` errors are returned (`?`) instead of panicking.
  Every reachable failure comes from the same path resolution that runs first,
  so the integration test exercises it through the `agentDir` error.
- `streamFn` wires settings timeouts, provider retry, and the
  provider-retry notifier; `convertToLlm` honors `images.blockImages`.
- `websocketConnectTimeoutMs` follows `??`: an explicit request value wins,
  otherwise the setting is read and an invalid setting fails the stream with
  `Invalid websocketConnectTimeoutMs setting: <json>`. Tests:
  `stream_function_forwards_websocket_connect_timeout_from_settings`,
  `stream_function_fails_when_the_websocket_connect_timeout_setting_is_invalid`.
- The extension runner ref is created by the SDK and populated by `AgentSession`.
  The same ref backs `onPayload` (`before_provider_request`), `onResponse`
  (`after_provider_response`), and `transformContext` (`context`), matching the
  source closure reads. Test: `create_agent_session_wires_extension_provider_hooks`
  (response/context handlers, patched context reaches the provider). Old-only before_provider_headers subscription/transform removed: zero frozen-package occurrences. Its exclusive handler/assertion is retired; real context/response assertions remain exact.
- `multiAgentAgentId`, `multiAgentParentSessionId`, `multiAgentRequiresAgentId`, and
  `supervisorDecisionRequester` are forwarded to `AgentSessionConfig`. Proof:
  `create_agent_session_forwards_multi_agent_identity_to_extensions` (tests/sdk.rs)
  reads the three identity carriers back through the extension context.
- `extensions_result` comes from the resource loader.

## Stream-option audit (2026-10-25)

Actual frozen SDK + faux provider differential found native overwrote request headers/environment with auth values, omitted attribution/session headers and retained caller cwd. Auth now resolves first, request environment overrides auth keys, ordered header merging uses the existing attribution helper, and SDK cwd replaces caller cwd in the existing extra-options carrier. No new API/dependency. RED `/tmp/ext-api2-sdk-stream-red5.log` compares real captured provider inputs; post-commit GREEN pending. Earlier fixture setup errors are not RED evidence; an erroneous exitCode-only oracle catch left readline open, so its job and all children were terminated and verified gone. Oracle fatal errors now exit explicitly.

## Gaps

- `base_tools` remains the injected tool override seam; when empty, `AgentSession`
  builds `createAllToolDefinitions` from current settings.
- `multiAgentStore`, `onSharedChannelMessageDelivered`, and `sessionStartEvent` are
  forwarded to `AgentSession`; mutation-target resolver binding remains in `main`
  (gap).

## Q18 native settings ID adapter (2026-10-22)

Default provider/model IDs read native Settings and reject unpaired units
explicitly before the approved scalar metadata contract. Provider attribution
receives native stream session IDs without an intermediate encoded copy.
Coding-library compilation still blocks runtime fixtures; no PASS claimed.
