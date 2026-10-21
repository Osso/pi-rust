# settings-manager.ts traceability

## Phase1 repin in progress (4f9e6f8a4)

Governing contract: [settings-manager](../../specs/settings-manager.md). One-level overlay, literal shellPath, steering all, validated retry base/cap, percentage helper, idle and visibility APIs replace old behavior. RED evidence: `/tmp/pi-rust-repin-settings/red-settings.txt`, `red-differential.txt`, `red-validation.txt`. GREEN pending caller integration. Bidirectional proof now retains a real TS manager across Rust writes and vice versa; fixture-only raw producer steps are labeled. Added remaining resource/identity setters and invalid reload/recovery scenarios; snapshots cover all 123 frozen get/set/is methods (composite views plus explicit leaf getters).

Main explicitly stages old-only UI APIs (`tuiMode`, fullscreen exit/scrollbar, cache-miss notices, markdown mermaid) unchanged until coupled Phase3 consumer deletion. Do not count those as frozen parity. Retry runtime, percentage/idle compaction consumers, display hiding and relay runtime remain TODO. Historical proof below predates repin and is not new-pin proof.


Source: `packages/coding-agent/src/core/settings-manager.ts` (1442 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/core/settings_manager.rs`.

## Exported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `Settings`, settings interfaces | `Settings = Map<String, Value>`; typed `CompactionSettings`, `BranchSummarySettings`, `RetrySettings`, `TerminalSettings`, `ImageSettings`, `ThinkingBudgetsSettings`, `MarkdownSettings`, `WarningSettings`, `AgentProfileSettings`, `PackageSource`/`PackageFilter` | differential raw snapshots; focused `packages_migration_*`, `agent_profiles_*` |
| `SettingsScope` | `SettingsScope` | all tests |
| `SettingsStorage` | `SettingsStorage` trait (`with_lock`) | `FileSettingsStorage` exercised by every file test; `InMemorySettingsStorage` by `project_enabled_models_override_global` |
| `FileSettingsStorage` | `FileSettingsStorage` (proper-lockfile via `utils::lockfile`, 10 × 20 ms retry, `realpath:false`) | every workspace test; lock protocol shared with `rule_store`/`trust_manager` |
| `InMemorySettingsStorage` | `InMemorySettingsStorage` | `project_enabled_models_override_global` |
| `SettingsManager` | `SettingsManager` | 23 focused tests + 10 differential scenarios |
| `create` / `fromStorage` / `inMemory` | `create` / `from_storage` / `in_memory` | focused + differential |
| `migrateSettings` | `migrate_settings` | differential `migrations`; focused `legacy_llm_approved_*` |
| `getGlobalSettings`/`getProjectSettings`/`getMergedSettings` | same | every differential snapshot |
| `getAgentProfile` | `get_agent_profile` | focused `agent_profiles_*`; differential |
| `getPermissionPromptTool` | `get_permission_prompt_tool` | differential `invalid-stored-values` |
| approval policy/preset get/set | `get_approval_policy`, `get_approval_preset`, `set_approval_policy`, `set_approval_preset` | focused + differential `approval-sandbox` |
| sandbox profile get/set (configured/explicit/session/merged) | same | focused `sandbox_profile_*`; differential |
| `isProjectTrusted`/`setProjectTrusted` | same | focused `untrusted_*`; differential `project-trust` |
| `reload` | `reload` | focused `reload_*`; differential |
| `applyOverrides` | `apply_overrides` | differential |
| `flush` | `flush` (no-op; writes are synchronous) | differential flushes before file comparison |
| `drainErrors` | `drain_errors` | focused `drains_load_errors_once`; differential error scopes |
| every typed getter/setter listed in the source | same-name snake_case methods | differential snapshots after every step + focused tests |
| `randomUUID` for `trackingId` | `pi_ai::utils::uuid::random_uuid` | focused `analytics_generates_*` |
| `parseTimeoutSetting` | `parse_timeout_setting` | focused `http_idle_timeout_*`; differential |

## Branches covered

- `queueMode`→`steeringMode`, `websockets`→`transport`, skills-object migration,
  `llm-approved`→`llm-approved-deny`, `retry.maxDelayMs`→`retry.provider.maxRetryDelayMs`.
- Only explicitly modified fields (and nested keys) overwrite the current file; external edits to other
  fields survive. Nested merges keep unmodified sibling keys.
- Project settings are skipped when untrusted, reloaded when trust turns on, and writes throw
  `Project is not trusted; refusing to write project settings`.
- A project settings load error blocks project writes; a global load error blocks global writes.
- Reading never creates `.pi`; writing creates it.
- Invalid stored values: unknown enum values fall back exactly as the source (`regular`, `transcript`,
  `auto`, `default`, `streaming`, `ask`, `workspace-write`), invalid timeout values throw.
- Numeric setters clamp/floor like `Math.max`/`Math.min` (including `NaN` → JSON `null`).
- File output uses `JSON.stringify(value, null, 2)` ordering/indentation with a trailing newline added by
  the file write path, byte-identical in differential scenarios.

## Discrepancies / deviations

- **Synchronous writes.** The TypeScript manager queues writes on a promise; Rust writes during the
  setter. Observable contract (file after `flush`) is identical and the differential flushes before
  reading files. `flush` is a no-op.
- **Typed JSON boundary.** `Settings` must be a JSON object; the source would accept arrays/primitives in
  `migrateSettings` and later fail. Non-string/non-array values are excluded by typed getters; the source
  sometimes returns them raw (e.g. `getTransport`, `getSteeringMode`, `getWarnings`).
- **`getPackages` returns `Result`.** A malformed package entry is reported instead of being returned raw.
- **Error text.** `Invalid <setting> setting: <value>` uses JSON serialization for objects instead of
  JavaScript `String(value)`; the prefix and throw behavior match.
- **Environment branches.** `getExternalEditorCommand`'s `VISUAL`/`EDITOR` lookup and
  `getShowHardwareCursor`/`getClearOnShrink` env fallbacks are implemented, but tests cannot set process
  environment (`std::env::set_var` is unsafe and forbidden by the workspace lint). Configured-value paths
  are tested.
- **`retry.provider` spread.** A non-object provider (array) is treated as absent instead of spreading
  numeric keys; invalid input excluded by the typed boundary.

## Proof

- Focused: `cargo test -p pi-coding-agent --test settings_manager` (23 passed).
- Differential: `cargo test -p pi-coding-agent --test differential_settings_manager` (1 passed) via
  `oracle/coding-agent-settings-manager-oracle.ts`; 10 scenarios, each comparing every getter, global and
  project settings, merged settings, both file bodies, thrown steps, and drained error scopes after every
  step.
