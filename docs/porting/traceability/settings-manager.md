# settings-manager.ts

## Legacy renderer key persistence (2026-10-22)

Frozen settings-manager has no tuiMode/fullscreen known API. Removed typed getters/setters/enums for tuiMode, fullscreenExitOutput and fullscreenScrollbar; no migration strips or rewrites those keys. `differential_settings_manager::legacy_tui_mode_keys_remain_unknown_settings_on_load_and_save` compares complete scoped/effective settings and persisted files against frozen before flush, after unrelated theme save, after external edits/reload and after another save. It includes string, object, number, null and array legacy values. Before deletion this differential passed, proving ordinary unknown-field preservation already matches frozen; no persistence code change was needed. File fixtures are isolated; no user settings changed.
 traceability

## Q18 profile-key input

`get_agent_profile` accepts `&JsString`, trims native units and queries the configured native key; built-in profile selection alone compares scalar literals. After the source-backed public spawn regressions executed RED, the getter now returns `Option<JsValue>` and clones configured JSON directly; it no longer deserializes/drops the whole profile. Callers project only accessed fields. Built-in defaults serialize to that same native representation, without a parallel typed configured-profile authority. Frozen getter itself only clones configured/default values (lines 509–516), so malformed settings are not blanket-rejected; only errors at fields actually accessed may propagate. The two actual in-crate REDs cover numeric-model failure and preservation of Fresh context with unrelated malformed tools; both pass at `4636282b` (`agents-core-drift-incrate-green.log`). This is bounded profile behavior, not whole settings parity.

Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, `packages/coding-agent/src/core/settings-manager.ts`.
Rust: `crates/pi-coding-agent-settings/src/core/settings_manager.rs`. Governing [contract](../../specs/settings-manager.md); [percentage helper](compaction-threshold.md).

## Reminder settings delta

`be6413897` adds the optional `reminders.deliverySessionId` type and reads it through `getMergedSettings`; it adds no settings setter. Rust retains the existing lossless settings object and provides `get_reminders_delivery_session_id() -> Option<&JsString>`. The native focused test covers trusted/project merge, untrusted project exclusion, lone-surrogate preservation, and unrelated global writes. The existing bidirectional frozen settings fixture now includes reminder objects in both scopes and checks their persistence through TS and Rust writes; it uses the common-pin oracle, whose generic merge and persistence logic applies to this field. The delta also contains unrelated settings changes outside this slice.

## Current status

**Done at the user-observable parity bar (2026-10-25).** Successor whole-module audit covers file/memory storage, migrations, scoped clone/merge/read/write/error queues, all getters/setters and shared drain. Existing five frozen differential tests pass at parent `3672fd002` (`/tmp/settings-models-settings-differential.log`), including exact file text, stale/concurrent writers, reciprocal TS/Rust writes and queued ordering. Successful persistence algorithms and serialization remain unchanged by the successor error-only fixes. Native error REDs reproduce directory-as-settings reads during construction and queued persistence and invalid timeout text. The corrections use Node read/open errors and JS String conversion. Historical phase notes below describe earlier checkpoints, not remaining blockers. Consumers have separate coverage rows.

DISCREPANCY (preserved): frozen `deepMergeSettings` promises recursive merging in its comment but merges only one object level; nested grandchildren are replaced, as the existing differential and native test assert.

Gap: adversarial/wrong-typed JSON, numeric extremes, unsupported OS/runtime fault shapes and scheduling races follow the repository parity bar; existing lossless carriers remain intact.
Renderer-mode APIs were removed as recorded above. Coupled cache/selector deletion now also removes get/set_show_cache_miss_notices, get/set_mermaid_rendering_mode, MermaidRenderingMode and MarkdownSettings.mermaid; raw legacy JSON keys remain ordinary unknown data rather than being scrubbed. [Removal classification](cache-stats.md). PackageFilter.autoload was removed in the successor persistence correction; any legacy JSON key remains ordinary unknown data.

## Exported types / lifecycle

| Frozen export | Literal Rust representation | Proof / explicit boundary |
| --- | --- | --- |
| `Settings` | ordered lossless `JsObject` (`Settings`) | every snapshot; unknown root/nested and relay preserved by bidirectional fixture |
| `CompactionSettings`, `BranchSummarySettings`, `RetrySettings`, `ProviderRetrySettings` | matching option-bearing structs; required effective view structs | leaf getters/views; defaults and numeric validation differential |
| `TerminalSettings`, `ImageSettings`, `ThinkingBudgetsSettings`, `WarningSettings` | matching option-bearing structs | snapshots and numeric/media setters |
| `MarkdownSettings` | struct with codeBlockIndent; old-only mermaid field removed | code indent observed; former renderer API removed |
| `AgentProfileThinkingLevel`, `AgentProfileSettings` | corresponding Rust enum/struct; native context enum | default profiles, project agent object replacement, null fallback |
| `PermissionRulesSettings` | raw `Settings["permissionRules"]` object, not a separate Rust named type | concrete allow-map overlay assertions |
| `DefaultProjectTrust`, `TransportSetting` | native trust enum and `pi_ai::types::Transport` | trust/scoped read/write and transport migrations |
| `PackageSource` | native enum/name/filter struct | known resource arrays; original property order and unknown keys retained by ModelProperties |
| `SettingsScope`, `SettingsManagerCreateOptions`, `SettingsError` | native enum/options/error record | scope observations, trusted/untrusted reads and drained error scopes |
| `SettingsStorage`, `FileSettingsStorage`, `InMemorySettingsStorage` | native trait, file and memory backends | isolated file tests plus native in-memory/from-storage tests; lock retry policy source-inspected, not new timing proof |
| `SettingsManager.create/fromStorage/inMemory` | corresponding native constructors | file and memory tests |
| `reload`, `applyOverrides`, `flush`, `drainErrors` | corresponding native methods | differential reload/error recovery, overrides, post-flush files; FIFO queued snapshot, same-turn setter/external-write ordering regression |

## Getter/setter observations

All 70 frozen getters/is methods are observed in `snapshot`; all 53 setters have oracle dispatch. Scalar leaf getters are observed separately even when a composite getter errors. Scenarios exercise source setters; analytics-generated UUID is covered natively rather than equating random UUID bytes across runtimes. Fixture-only producer writes are explicitly separate from product setters.

| Frozen method | Rust method | Behavioral observation |
| --- | --- | --- |
| `getGlobalSettings` | `get_global_settings` | effective getter snapshot |
| `getProjectSettings` | `get_project_settings` | effective getter snapshot |
| `getMergedSettings` | `get_merged_settings` | effective getter snapshot |
| `getAgentProfile` | `get_agent_profile` | effective getter snapshot |
| `getPermissionPromptTool` | `get_permission_prompt_tool` | effective getter snapshot |
| `getApprovalPolicy` | `get_approval_policy` | effective getter snapshot |
| `getApprovalPreset` | `get_approval_preset` | effective getter snapshot |
| `setApprovalPolicy` | `set_approval_policy` | API mutation then effective snapshot and persisted file comparison |
| `setApprovalPreset` | `set_approval_preset` | API mutation then effective snapshot and persisted file comparison |
| `getSandboxProfile` | `get_sandbox_profile` | effective getter snapshot |
| `getConfiguredSandboxProfile` | `get_configured_sandbox_profile` | effective getter snapshot |
| `getExplicitSandboxProfile` | `get_explicit_sandbox_profile` | effective getter snapshot |
| `getSessionSandboxProfile` | `get_session_sandbox_profile` | effective getter snapshot |
| `setSessionSandboxProfile` | `set_session_sandbox_profile` | API mutation then effective snapshot and persisted file comparison |
| `setSandboxProfile` | `set_sandbox_profile` | API mutation then effective snapshot and persisted file comparison |
| `isProjectTrusted` | `is_project_trusted` | effective getter snapshot |
| `setProjectTrusted` | `set_project_trusted` | API mutation then effective snapshot and persisted file comparison |
| `getLastChangelogVersion` | `get_last_changelog_version` | effective getter snapshot |
| `setLastChangelogVersion` | `set_last_changelog_version` | API mutation then effective snapshot and persisted file comparison |
| `getSessionDir` | `get_session_dir` | effective getter snapshot |
| `getDefaultProvider` | `get_default_provider` | effective getter snapshot |
| `getDefaultModel` | `get_default_model` | effective getter snapshot |
| `setDefaultProvider` | `set_default_provider` | API mutation then effective snapshot and persisted file comparison |
| `setDefaultModel` | `set_default_model` | API mutation then effective snapshot and persisted file comparison |
| `setDefaultModelAndProvider` | `set_default_model_and_provider` | API mutation then effective snapshot and persisted file comparison |
| `getSteeringMode` | `get_steering_mode` | effective getter snapshot |
| `setSteeringMode` | `set_steering_mode` | API mutation then effective snapshot and persisted file comparison |
| `getFollowUpMode` | `get_follow_up_mode` | effective getter snapshot |
| `setFollowUpMode` | `set_follow_up_mode` | API mutation then effective snapshot and persisted file comparison |
| `getThemeSetting` | `get_theme_setting` | effective getter snapshot |
| `getTheme` | `get_theme` | effective getter snapshot |
| `setTheme` | `set_theme` | API mutation then effective snapshot and persisted file comparison |
| `getDefaultThinkingLevel` | `get_default_thinking_level` | effective getter snapshot |
| `setDefaultThinkingLevel` | `set_default_thinking_level` | API mutation then effective snapshot and persisted file comparison |
| `getTransport` | `get_transport` | effective getter snapshot |
| `setTransport` | `set_transport` | API mutation then effective snapshot and persisted file comparison |
| `getCompactionEnabled` | `get_compaction_enabled` | effective getter snapshot |
| `getIdleCompactionEnabled` | `get_idle_compaction_enabled` | effective getter snapshot |
| `setCompactionEnabled` | `set_compaction_enabled` | API mutation then effective snapshot and persisted file comparison |
| `getCompactionReserveTokens` | `get_compaction_reserve_tokens` | effective getter snapshot |
| `getCompactionKeepRecentTokens` | `get_compaction_keep_recent_tokens` | effective getter snapshot |
| `getCompactionSettings` | `get_compaction_settings` | effective getter snapshot |
| `getBranchSummarySettings` | `get_branch_summary_settings` | effective getter snapshot |
| `getBranchSummarySkipPrompt` | `get_branch_summary_skip_prompt` | effective getter snapshot |
| `getRetryEnabled` | `get_retry_enabled` | effective getter snapshot |
| `setRetryEnabled` | `set_retry_enabled` | API mutation then effective snapshot and persisted file comparison |
| `getRetrySettings` | `get_retry_settings` | effective getter snapshot |
| `getHttpIdleTimeoutMs` | `get_http_idle_timeout_ms` | effective getter snapshot |
| `setHttpIdleTimeoutMs` | `set_http_idle_timeout_ms` | API mutation then effective snapshot and persisted file comparison |
| `getProviderRetrySettings` | `get_provider_retry_settings` | effective getter snapshot |
| `getWebSocketConnectTimeoutMs` | `get_websocket_connect_timeout_ms` | effective getter snapshot |
| `getHideThinkingBlock` | `get_hide_thinking_block` | effective getter snapshot |
| `getHideToolOutput` | `get_hide_tool_output` | effective getter snapshot |
| `setHideToolOutput` | `set_hide_tool_output` | API mutation then effective snapshot and persisted file comparison |
| `getHideCoordinationMessages` | `get_hide_coordination_messages` | effective getter snapshot |
| `setHideCoordinationMessages` | `set_hide_coordination_messages` | API mutation then effective snapshot and persisted file comparison |
| `getExternalEditorCommand` | `get_external_editor_command` | effective getter snapshot |
| `setHideThinkingBlock` | `set_hide_thinking_block` | API mutation then effective snapshot and persisted file comparison |
| `getShellPath` | `get_shell_path` | effective getter snapshot |
| `setShellPath` | `set_shell_path` | API mutation then effective snapshot and persisted file comparison |
| `getQuietStartup` | `get_quiet_startup` | effective getter snapshot |
| `setQuietStartup` | `set_quiet_startup` | API mutation then effective snapshot and persisted file comparison |
| `getDefaultProjectTrust` | `get_default_project_trust` | effective getter snapshot |
| `setDefaultProjectTrust` | `set_default_project_trust` | API mutation then effective snapshot and persisted file comparison |
| `getShellCommandPrefix` | `get_shell_command_prefix` | effective getter snapshot |
| `setShellCommandPrefix` | `set_shell_command_prefix` | API mutation then effective snapshot and persisted file comparison |
| `getNpmCommand` | `get_npm_command` | effective getter snapshot |
| `setNpmCommand` | `set_npm_command` | API mutation then effective snapshot and persisted file comparison |
| `getCollapseChangelog` | `get_collapse_changelog` | effective getter snapshot |
| `setCollapseChangelog` | `set_collapse_changelog` | API mutation then effective snapshot and persisted file comparison |
| `getEnableInstallTelemetry` | `get_enable_install_telemetry` | effective getter snapshot |
| `setEnableInstallTelemetry` | `set_enable_install_telemetry` | API mutation then effective snapshot and persisted file comparison |
| `getEnableAnalytics` | `get_enable_analytics` | effective getter snapshot |
| `getTrackingId` | `get_tracking_id` | effective getter snapshot |
| `setEnableAnalytics` | `set_enable_analytics` | API mutation then effective snapshot and persisted file comparison |
| `getPackages` | `get_packages` | effective getter snapshot |
| `setPackages` | `set_packages` | API mutation then effective snapshot and persisted file comparison |
| `setProjectPackages` | `set_project_packages` | API mutation then effective snapshot and persisted file comparison |
| `getExtensionPaths` | `get_extension_paths` | effective getter snapshot |
| `getDisabledExtensions` | `get_disabled_extensions` | effective getter snapshot |
| `setExtensionPaths` | `set_extension_paths` | API mutation then effective snapshot and persisted file comparison |
| `setProjectExtensionPaths` | `set_project_extension_paths` | API mutation then effective snapshot and persisted file comparison |
| `getSkillPaths` | `get_skill_paths` | effective getter snapshot |
| `setSkillPaths` | `set_skill_paths` | API mutation then effective snapshot and persisted file comparison |
| `setProjectSkillPaths` | `set_project_skill_paths` | API mutation then effective snapshot and persisted file comparison |
| `getPromptTemplatePaths` | `get_prompt_template_paths` | effective getter snapshot |
| `setPromptTemplatePaths` | `set_prompt_template_paths` | API mutation then effective snapshot and persisted file comparison |
| `setProjectPromptTemplatePaths` | `set_project_prompt_template_paths` | API mutation then effective snapshot and persisted file comparison |
| `getThemePaths` | `get_theme_paths` | effective getter snapshot |
| `setThemePaths` | `set_theme_paths` | API mutation then effective snapshot and persisted file comparison |
| `setProjectThemePaths` | `set_project_theme_paths` | API mutation then effective snapshot and persisted file comparison |
| `getEnableSkillCommands` | `get_enable_skill_commands` | effective getter snapshot |
| `setEnableSkillCommands` | `set_enable_skill_commands` | API mutation then effective snapshot and persisted file comparison |
| `getThinkingBudgets` | `get_thinking_budgets` | effective getter snapshot |
| `getShowImages` | `get_show_images` | effective getter snapshot |
| `setShowImages` | `set_show_images` | API mutation then effective snapshot and persisted file comparison |
| `getImageWidthCells` | `get_image_width_cells` | effective getter snapshot |
| `setImageWidthCells` | `set_image_width_cells` | API mutation then effective snapshot and persisted file comparison |
| `getClearOnShrink` | `get_clear_on_shrink` | effective getter snapshot |
| `setClearOnShrink` | `set_clear_on_shrink` | API mutation then effective snapshot and persisted file comparison |
| `getShowTerminalProgress` | `get_show_terminal_progress` | effective getter snapshot |
| `setShowTerminalProgress` | `set_show_terminal_progress` | API mutation then effective snapshot and persisted file comparison |
| `getImageAutoResize` | `get_image_auto_resize` | effective getter snapshot |
| `setImageAutoResize` | `set_image_auto_resize` | API mutation then effective snapshot and persisted file comparison |
| `getBlockImages` | `get_block_images` | effective getter snapshot |
| `setBlockImages` | `set_block_images` | API mutation then effective snapshot and persisted file comparison |
| `getEnabledModels` | `get_enabled_models` | effective getter snapshot |
| `setEnabledModels` | `set_enabled_models` | API mutation then effective snapshot and persisted file comparison |
| `getDoubleEscapeAction` | `get_double_escape_action` | effective getter snapshot |
| `setDoubleEscapeAction` | `set_double_escape_action` | API mutation then effective snapshot and persisted file comparison |
| `getTreeFilterMode` | `get_tree_filter_mode` | effective getter snapshot |
| `setTreeFilterMode` | `set_tree_filter_mode` | API mutation then effective snapshot and persisted file comparison |
| `getShowHardwareCursor` | `get_show_hardware_cursor` | effective getter snapshot |
| `setShowHardwareCursor` | `set_show_hardware_cursor` | API mutation then effective snapshot and persisted file comparison |
| `getEditorPaddingX` | `get_editor_padding_x` | effective getter snapshot |
| `setEditorPaddingX` | `set_editor_padding_x` | API mutation then effective snapshot and persisted file comparison |
| `getOutputPad` | `get_output_pad` | effective getter snapshot |
| `setOutputPad` | `set_output_pad` | API mutation then effective snapshot and persisted file comparison |
| `getAutocompleteMaxVisible` | `get_autocomplete_max_visible` | effective getter snapshot |
| `setAutocompleteMaxVisible` | `set_autocomplete_max_visible` | API mutation then effective snapshot and persisted file comparison |
| `getCodeBlockIndent` | `get_code_block_indent` | effective getter snapshot |
| `getWarnings` | `get_warnings` | effective getter snapshot |
| `setWarnings` | `set_warnings` | API mutation then effective snapshot and persisted file comparison |

## Branch matrix

| Contract | Test / exact evidence | Remaining boundary |
| --- | --- | --- |
| One-level object overlay | `nested_scope_overlay_replaces_grandchildren`; differential `one-level-overlay` | source misleading recursive comment recorded; no recursive semantic deviation |
| agents/retry.provider/permissionRules.allow | global/project concrete grandchildren; arrays/null/session overrides | invalid non-object JS runtime values outside declared types not universally modeled |
| Unknown properties + competing writes | `settings_manager_bidirectional_stale_writers_preserve_unknown_fields`: persistent TS/Rust managers, raw producer, both directions; overlapping process writes to distinct terminal children | after-flush observations do not prove automatic native event-loop drain |
| Unchanged migrations | differential `migrations`; native legacy preset/package cases | malformed JS-only values not exhaustive |
| Scope precedence/trust/errors | differential `project-trust`, `approval-sandbox`, `reload-errors-and-recovery`; native blocked writes | execution-time trust recheck and failed-task draining covered by queued-ordering scenarios |
| Defaults/new validation | differential `repin-validation-and-catalog`, native exact error tests | retry runtime policy not settings parity |
| Literal shellPath | native `shell_path_is_literal`, differential paths | no shell consumer claim |
| Idle/hide flags | new getters/setters, defaults and bidirectional effective flags | scheduler/transcript consumers TODO |

## Explicit boundaries / consumers TODO

- Settings module has no retry scheduling, percentage-triggered compaction, idle scheduling, transcript suppression, or relay startup. B2/B3/relay proof remains TODO.
- Rust JSON boundary cannot represent `NaN`/infinities; direct JS-invalid runtime types and non-object root documents are not claimed equivalent. Typed core exclusion is tracked in project rules.
- Error scope/throw behavior is compared. New retry/percentage errors use JavaScript string conversion; timeout errors now use lossless JavaScript String conversion (successor correction).
- RED same-turn setter/external-write/flush mismatch reproduced at e2d89154 + fixture correction (`red-queued-ordering.txt`). Setters now queue scoped snapshots; mutable `flush` drains FIFO and `reload` first flushes. Shared guard release now schedules an owned FIFO worker without a runtime dependency. AgentSession's persistent setters/accessor, interactive shared-lock consumers, and package-manager adapter use this boundary. Standalone mutable managers and raw mutex locks do not self-schedule; explicit `flush`/`reload` retain synchronous barrier behavior.
- Configured external editor is observed. Environment defaults are sandbox-isolated; broader host-locale/environment branches are not established by these fixtures.

## Shared-drain slice (2026-10-21)

`tests/settings_manager_drain.rs` exercises the production `SharedSettingsManager` plus `lock_settings_manager` boundary with instrumented real file storage and bounded channel completion, never sleep-only timing. Cases: guard release without flush/runtime plus owner-drop lifetime and fresh-manager reconstruction; global/project/global immutable FIFO snapshots and subsequent-burst scheduling; real storage failure recorded once while later scope continues; synchronous flush with no replay after scheduled owner exits; execution-time project trust rejection. Initial RED: four automatic cases fail (no scheduling); existing explicit flush case passes. Initial implementation GREEN: 5/5. Dropped-scheduling mutant fails on missing persistence; reversed-FIFO mutant fails on `second` instead of `first`. Both restored exactly. Commands/revisions and subsequent proof are recorded in `/tmp/pi-rust-repin-settings-drain/ledger.md`; no whole-module coverage promotion or final verification implied.

## Proof ledger

`/tmp/pi-rust-repin-settings/ledger.md`; RED artifacts `red-settings.txt`, `red-differential.txt`, `red-validation.txt`. Focused development tests only; no check/clippy/readability/final acceptance gate executed by this slice.

## Drain failure correction (2026-10-21)

- Frozen `enqueueWrite` lines 709–720 catch each task failure and continue; Rust `flush` now catches unwinds at that same per-snapshot boundary, records the original scoped panic, and continues without rollback/retry. `panicking_write_is_recorded_without_dropping_later_snapshots_or_latching_drain` injects a one-shot panic inside real file storage's lock, observes later project/global snapshots, drains one error, then persists/reconstructs another burst. RED at `dc0aea52` times out on the later project snapshot.
- `SettingsDrainGuard::drop` clears scheduling under the worker-held mutex, both normally and during unwind. `spawn_settings_drain` uses `Builder::spawn`; OS failure clears the reservation and records an explicit error against the first queued scope without taking snapshots. Thread creation failure is source-proven only: no spawner seam or host resource exhaustion. Queue remains available to a later shared guard release or explicit flush.
- Raw mutation audit: package-manager CLI trust setters and resource-loader trust changes only update/reload trust state, never queue saves. SDK session sandbox override is session-only. Resource-loader/AgentSession reload synchronously flushes earlier snapshots. SDK settings example's persistent setter immediately calls explicit flush under its raw guard. No audited caller needs rerouting. Shared persistent callers without an explicit barrier must still use the helper.

## Q18 package contract boundary (2026-10-22)

`PackageSource`, `PackageFilter` resource arrays/source, `get_npm_command`, and
package-owned settings snapshots now use `JsString`. `get_disabled_extensions`
was widened in the resource-loader slice. The settings JSON authority is still
scalar serde_json by explicit parent scope; a separately queued native-authority
slice must fix persisted raw-unit ingress. No parallel native cache is added.
`lossless_package_contracts` proves the intended in-memory source/pattern contract
when executable; currently written but coding-library compilation blocks runtime
proof. Snapshot arrays refuse malformed mixed-type fields wholesale; invalid JS
runtime types remain excluded, not partial-filter parity.

Successor GREEN at `4e68c35cc`: 30 settings native, six shared-drain and one lossless persistence tests; logs `/tmp/settings-models-settings-{native,drain,lossless}-green.log`. Workspace/all-targets check passes (`/tmp/settings-models-successor-check-2.log`). Coverage summary corrected from 641/8 to 642/7 after the initial inventory correctly rejected the stale displayed counts.

## Package-filter persistence correction (2026-10-25)

Native RED `/tmp/settings-models-package-filter-red.log` proves get_packages→set_packages previously dropped an unknown property and moved source before skills. The existing PackageFilter now reuses `pi_ai::types::ModelProperties` directly: its lossless JsObject extras and original key-order metadata are retained, while typed fields own known values. No new key-order authority or parallel known-value cache was introduced. Typed source edits update the original position; newly created known fields append. The old-only autoload field is now unknown retained JSON, not a native setting. Removing Eq from the package settings snapshot is mechanical fallout of retained arbitrary JSON.

One native round-trip test asserts exact file bytes before and after a typed source edit. The existing reciprocal stale-writer fixture now has skills-before-source plus an unknown property; TS writes and round-trips it, Rust reads and rewrites byte-identically, then TS reads/rewrites those native bytes. All five settings differentials pass (`/tmp/settings-models-settings-reciprocal-final.log`) and the native regression passes (`/tmp/settings-models-package-filter-green.log`).

## File decoding (compat re-audit 2026-10-26)

Source reads with `readFileSync(path, "utf-8")` (settings-manager.ts:282); native reads bytes and decodes with U+FFFD replacement of invalid sequences (same rule as Node), so a stray Latin-1 byte no longer drops the file. RED/GREEN: `lossless_settings::invalid_utf8_bytes_decode_like_node_without_dropping_the_scope` (/tmp/config-utf8-red*.log, /tmp/config-utf8-green.log).
