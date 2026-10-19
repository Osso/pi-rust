# agent-session-services.ts traceability

Rust: `crates/pi-coding-agent/src/core/agent_session_services.rs`.

| TS | Rust | Tests |
| --- | --- | --- |
| `createAgentSessionServices` | `create_agent_session_services` | `tests/sdk.rs::services_and_from_services_create_a_working_session`, `tests/agent_session_services.rs` (3 tests) |
| `createAgentSessionFromServices` | `create_agent_session_from_services` | `tests/sdk.rs` |
| `AgentSessionServices` | `AgentSessionServices` | — |
| `AgentSessionRuntimeDiagnostic` | same | `tests/main.rs::diagnostics_are_serialized_with_the_ts_type_field` |
| `applyExtensionFlagValues` | `apply_extension_flag_values` | `extension_flags_are_applied_and_unknown_flags_reported`, `string_extension_flag_without_a_value_is_rejected` |
| pending provider registration flush | `register_pending_providers` | covered by resource-loader/extension tests; no dedicated provider fixture |

## Behavior

- `cwd`/`agentDir` resolve through `utils::paths::resolve_path` with the process cwd.
- A caller-supplied `resource_loader` wins; otherwise `DefaultResourceLoaderOptions`
  builds a `DefaultResourceLoader` with the resolved cwd/agent dir and the same
  settings manager.
- `resource_loader_reload_options` is forwarded through
  `ResourceLoader::reload_with_options` (`reload(options)` in the source).
- Pending extension provider registrations are registered on the model runtime,
  then cleared; failures become `Extension "<path>" error: <message>` diagnostics.
- Extension flag values apply in argument order; unknown flags and missing string
  values produce the source error strings.
- `model_runtime_signal` is forwarded to `ModelRuntime::create`.

## Gaps

- None for the option surface: `multiAgentStore`, `onSharedChannelMessageDelivered`,
  `customTools`, and `sessionStartEvent` are forwarded to `AgentSession`.
