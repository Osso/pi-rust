# sdk.ts traceability

Rust: `crates/pi-coding-agent-core/src/core/sdk.rs`.
Frozen `4f9e6f8a4` whole-module audit closed 2026-10-25. [Current proof](../ext-api-repin-proof.md) renews all 15 SDK behavioral/differential tests and supersedes historical pending labels below.

Q18: convert-to-LLM callback propagates native AgentError from core conversion before optional image blocking; invalid typed provider context is never omitted or Display-encoded. SDK cwd/agent_dir/control_db_path options, result fallback message and errors now use JsString. Native resolution and node_join preserve path units through auth/models path construction, SettingsManager/SessionManager/resource-loader adapters and AgentSession config; no SDK encoding boundary. `lossless_agent_session_paths` adds SDK reopen/agent-dir assertions. Execution remains blocked by scalar model-storage/settings/resource-loader dependencies and coding-library callers. `sdk-path-red.jsonl` records the blocked target before SDK migration; no runtime parity claim.

| TS | Rust | Tests |
| --- | --- | --- |
| `createAgentSession` | `create_agent_session` | `tests/sdk.rs` (4), `tests/differential_agent_session.rs` |
| `createAgentSessionWithInternalOptions` | same | — |
| `CreateAgentSessionOptions` | same, plus Rust-native `base_tool_definitions`, `custom_tool_definitions`, `multi_agent_store`, `disable_runtime_coordination_inbound` carriers | `create_agent_session_forwards_observer_coordination_options` |
| `CreateAgentSessionResult` | same (`extensions_result: LoadExtensionsResult`) | export/runtime tests |
| `rulesScopeForRuntimeRole` | `rules_scope_for_runtime_role` | DefaultResourceLoader tests |
| `restorePersistedSessionSettings` | inline restore of sandbox profile | `create_agent_session_*` |
| Re-exported tool factories / `withFileMutationQueue` | direct SDK forwards | `core_and_sdk_barrels_forward_frozen_public_contracts` |

## Behavior

- Model resolution: explicit model, persisted session settings, then
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
  `Invalid websocketConnectTimeoutMs setting: <String(value)>` (frozen settings-manager.ts:204).
  Final-gate fixture corrected after 4e68c35cc restored this interpolation; string values have no JSON quotes. Tests:
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

## No-auth startup/restart falsification (2026-10-25)

Parent requested an absent-model representation only if actual frozen startup/persistence differed. Frozen agent.ts:85 substitutes DEFAULT_MODEL even when SDK's local model is undefined; AgentSession.model returns that state directly. The real frozen SDK/runtime and native SDK/runtime both report unknown/unknown, leave entry arrays empty, and persist unknown/unknown + thinking off before an injected post-persistence stop hook. Only the established source-package/binary-package root in the full startup guidance text is normalized. `/tmp/ext-api2-no-model-probe2.log` records the initial package-root-only mismatch, not an absent-model RED. No Option<Model> state migration is justified or implemented. Final focused proof pending at commit.

## Startup ordering audit (2026-10-25)

Frozen reloads default resources, builds existing-session context, then restores persisted settings. Native restored settings before extension factories ran, so factories saw an already-cleared session sandbox override. Moved the existing restoration block to frozen order. RED `/tmp/ext-api2-sdk-order-red2.log`: native factory sees null, frozen sees full-access; both end at null. The actual default resource loader and inline extension factory are exercised, not a mock ordering assertion. GREEN pending.

## Historical model restoration audit (2026-10-25)

Whole-module comparison found an old-only fallback from persisted metadata to the session context's last `model_change`. Frozen SDK restores only metadata; historical entries without metadata must use configured defaults. Removed that fallback. Actual frozen/native RED `/tmp/ext-api3-sdk-legacy-red.log` selects `preferred` versus native `historical` from the same two-model catalog and session entry. `historical_model_entries_do_not_override_settings_without_persisted_metadata` covers the ordinary in-memory/imported-session boundary; post-commit GREEN pending.

## Gaps

- Rust-native base_tools is an injection seam; when empty, AgentSession builds current built-in definitions. Tool's erased TS factory-result union uses existing native tool traits, not a new unused enum.
- SDK forwards the multi-agent store, coordination callback and session-start metadata. Mutation-target resolver binding is outside this frozen SDK module, not a missing SDK branch.
- Gap: adversarial shapes, exceptional OS/runtime failures and numeric extremes are outside the current parity bar.

## Q18 native settings ID adapter (2026-10-22)

Default provider/model IDs read native Settings and reject unpaired units
explicitly before the approved scalar metadata contract. Provider attribution
receives native stream session IDs without an intermediate encoded copy.
Coding-library compilation still blocks runtime fixtures; no PASS claimed.
