# resource-loader.ts traceability

Source: `packages/coding-agent/src/core/resource-loader.ts` (1270 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent-tools/src/core/resource_loader.rs`.

Tests: `crates/pi-coding-agent/tests/resource_loader.rs` (25 behavioral cases) and
`crates/pi-coding-agent/tests/differential_resource_loader.rs` (rules differential + eight
context-file fixture trees compared against the real pinned TypeScript module).

## Ported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| private `firstPartyExtensionName` | `first_party_extension_name` | `names_first_party_and_inline_extension_paths` |
| private `inlineExtensionName` | `inline_extension_name` | same |
| private `resolvePromptInput` | `resolve_prompt_input` | `resolves_prompt_input_files_and_literals` |
| `loadRulesFilesFromDir` | `load_rules_files_from_dir` | differential `coding-agent-resource-loader-oracle.ts` (order, trim, empty-file skip, missing dir, non-`.md`, Unicode collation) |
| `loadRulesFromDir` | `load_rules_from_dir` | same differential request |
| `RulesScope` | `RulesScope` | `rules_load_in_sorted_order_and_respect_scope` |
| private `loadContextFilesFromDir` | `load_context_files_from_dir` | `agents_context_files_layer_with_overrides_and_memory`, `context_file_variants_dedupe_and_directories_are_ignored`, differential fixtures |
| private `loadInstructionContextFilesFromDirs` | `load_instruction_context_files_from_dirs` | `agents_family_suppresses_claude_family_across_directories`, `claude_family_loads_when_no_agents_family_exists` |
| private `appendUniqueContextFiles` | `append_unique_context_files` | same tests (realpath dedupe) |
| private `listAncestorDirs` | `list_ancestor_dirs` | layered fixtures |
| `findShadowedContextFile` | `find_shadowed_context_file` | `shadowed_worktree_context_files_are_deduplicated`, `bare_layout_container_context_is_not_shadowed`, `submodule_and_missing_gitdir_climb_normally`, differential worktree/bare/sibling/submodule fixtures |
| `loadProjectContextFiles` | `load_project_context_files` | differential `load_project_context_files_matches_the_pinned_typescript_source` (8 fixture trees) + focused cases |
| `DefaultResourceLoader` constructor/state | `DefaultResourceLoader::new`, `ResourceLoaderState` | `starts_empty_and_discovers_skills_and_prompts` |
| `getExtensions/getSkills/getPrompts/getThemes/getAgentsFiles/getRulesFiles/getRulesContent/getSystemPrompt/getSystemPromptSource/getAppendSystemPrompt/getAppendSystemPromptSources` | same-named trait methods | every behavioral test asserts loaded state |
| `extendResources` | `extend_resources` | `extend_resources_loads_skills_and_prompts_with_metadata` |
| `loadProjectTrustExtensions` | `load_project_trust_extensions` | `default_project_trust_loader_forces_untrusted_and_loads_inline_factories` |
| `reload` | `reload`, `reload_with` | `rules_reload_from_disk`, trust/source/override tests |
| private `resolveExtensionLoadPath` | `resolve_extension_load_path` | pre-trust reuse path in `reload` |
| private `loadCurrentExtensionSet` | `load_current_extension_set` | trust bootstrap test |
| private `loadFinalExtensionSet` | `load_final_extension_set` | trust bootstrap + conflict tests |
| private `addExtensionConflictDiagnostics` | `add_extension_conflict_diagnostics` | `extension_conflicts_report_tools_and_flags` |
| private `detectExtensionConflicts` | `detect_extension_conflicts` | same (exact `Tool "..." conflicts with ...` / `Flag "--..." conflicts with ...` text) |
| private `mapSkillPath` | `map_skill_path` | `project_resources_win_name_collisions` |
| private `normalizeExtensionPaths` | `normalize_extension_paths` | `extend_resources_loads_skills_and_prompts_with_metadata` |
| private `updateSkillsFromPaths/updatePromptsFromPaths/updateThemesFromPaths` | same snake-case methods | extend/override/collision tests |
| private `applyExtensionSourceInfo` | `apply_extension_source_info` | `extension_conflicts_report_tools_and_flags` |
| private `findSourceInfoForPath` | `find_source_info_for_path` | extension/package metadata tests |
| private `getDefaultSourceInfoForPath` | `get_default_source_info_for_path` | `<inline:...>` source `"inline"`, agent/project root scopes |
| private `mergePaths` | `merge_paths` | collision tests (project-first canonical dedupe) |
| private `resolveResourcePath` | `resolve_resource_path` | all path-based tests |
| private `loadThemes/loadThemesFromDir/loadThemeFromFile` | same snake-case methods | theme collision and discovery tests |
| private `getDisabledExtensionNames/isExtensionPathDisabled/filterDisabledExtensionPaths/isExtensionFactoryDisabled` | same snake-case methods (shared helpers from `extensions::loader`) | `disabled_inline_extension_factories_are_skipped` |
| private `loadExtensionFactories` | `load_extension_factories` | conflict/trust/disabled tests |
| private `dedupePrompts` | `dedupe_prompts` | `project_resources_win_name_collisions` |
| private `dedupeThemes` | `dedupe_themes` | same |
| private `discoverSystemPromptFile/discoverAppendSystemPromptFile` | same snake-case methods | `system_and_append_prompt_discovery_and_sources` |
| private `isUnderPath` | `is_under_path` | source-info root tests |
| `ResourceLoader` interface | `ResourceLoader` trait (+`LoadedThemes`, `LoadedRulesFiles`, `LoadedPathSource`) | `StaticResourceLoader`/`DefaultResourceLoader` impls |
| `ResourceExtensionPaths`, `ResourceLoaderReloadOptions` | `ResourceExtensionPaths`, `ResolveProjectTrust` | extend/trust tests |
| `DefaultResourceLoaderOptions` | `DefaultResourceLoaderOptions` | all behavioral tests |
| `InlineExtension` | `InlineExtension` enum (`Factory` with optional `extensionPath`, `Named`) | disabled/trust/conflict tests |

## Deviations and gaps

- **TypeScript extension module loading** is the approved deviation (2026-10-18). `loadExtensionsCached`
  resolves factories through `ExtensionRegistry` keyed by resolved path or id; filesystem `.ts`/`.js`
  entries without a registered factory produce the source's
  `Extension does not export a valid factory function: {path}` diagnostic.
- `extendResources` is declared `void` in the source; the Rust trait method cannot propagate a
  `loadSkills`/`loadPromptTemplates` path error. Errors are dropped there, while `reload` propagates
  them. Recorded as a trait-shape gap, not a source behavior.
- Theme JSON parse/validation error text comes from the Rust theme loader (`ThemeError` Display), not
  Node's `JSON.parse` message. Diagnostics still carry the file path and warning type.
- `resolvePromptInput`/context-file warnings go to stderr without chalk colors; message text matches
  the source apart from the platform error string appended after `: `.
- `canonicalizePath` falls back to the input path when `realpath` fails; the source's `realpathSync`
  would throw and skip the file. Only reachable on races/permission errors.
- The loader owns an `ExtensionRegistry` option because Rust extensions are registered rather than
  imported.

## Discrepancies

None found in the ported branches; the context-file differential matched the pinned source for
ordinary repos, nested/sibling/bare worktrees, submodules, symlinked memory, override layering, and
directory candidates.

## Q18 native continuation (2026-10-22)

All stored prompt/rule/context content, path keys, inline names, override inputs,
diagnostics, and ResourceLoader results now use `JsString`. Native node path
operations preserve units until actual filesystem reads/stat/readdir; read bytes
use Node UTF-8 replacement behavior at that ingress. Locale sorting uses UTF-16
collation. `lossless_resource_loader` checks literal prompts, inline names, and
static getters with raw units; written but unexecuted.

replacement-resources-2: 20 remaining primary diagnostics, all stale package
resource/metadata contracts or the UI-owned native theme API. Coding tests remain
library-blocked. No runtime PASS or renewed frozen parity claim.

Settings `get_disabled_extensions` returns native strings, but its JSON ingress
remains scalar serde_json pending the separately assigned SettingsManager native
authority slice. This projection does not claim lossless persisted settings.

### Source-drift removals

Removed old-only AGENTS.override.md precedence and worktree-shadow suppression;
neither exists in common-pin resource-loader.ts:88–122 and no locked delta
authorizes them. Context candidates now match the frozen AGENTS/CLAUDE lists and
family selection. Deleted shadow-only tests; updated layered instructions
expectations. New frozen oracle comparison covers an override file beside regular
AGENTS files. Coding compilation still blocks its execution (no behavioral RED
claim). The now-unneeded native footer path migration is reverted completely.

`extend_resources` returns `Result` and propagates update failures rather than
discarding them. AgentSession forwards the error from resource discovery through
reload/bind_extensions. Invalid file-URL fixture asserts an observable failure;
written but unexecuted. These are source-drift removals, not approved deviations.

## Reload publication and rule-read audit (2026-10-25)

Actual frozen reload publishes extension/skill/prompt/theme/context stages in order, and earlier successful updates remain visible after a later failure. Native previously built an atomic replacement snapshot: prompt overrides saw the old skill set, and a rule-read error discarded new skills. Reload now publishes each stage without retaining state locks across callbacks. Rule reads report Node scandir/read text instead of Rust OS Display text. RED `/tmp/ext-api2-resource-state-red.log`: callback observations `[[],[first]]` and retained first differ from frozen `[[first],[second]]` and retained second; native EISDIR text also differs. Focused GREEN pending.

## Constructor error carrier

[Contract](../../specs/resource-loader-constructor-errors.md). Frozen :302–311 resolves cwd/agentDir and constructs settings/package manager without flattening errors. Native `DefaultResourceLoader::new` now returns CallbackError, resolves constructor paths fallibly without the old unresolved-input fallback, and preserves settings and package-manager error sources. SDK/services/supervisor and project-trust/create-settings adapters forward the same carrier. Other loader helpers are not revalidated by this constructor slice.

Actual frozen/native `resource_loader_constructor_nss_errors_keep_native_identity_through_callers` uses a private sequenced getpwuid_r interposer. Direct constructor/default trust loader fail immediately; SDK/services permit their preceding four successful path normalizations before NSS fails at loader construction. Source trust-loader reference is the actual constructor, since that adapter delegates directly to it before loading. Genuine RED `resource-loader-constructor-red.log`: all four native callers returned generic Error/message only instead of SystemError/code/errno/syscall/info. Initial GREEN `resource-loader-constructor-green-attempt.log`:1/1, four callers. Final `resource-loader-constructor-regressions.log` adds later embedded package-manager failures:1/1, eight source/native cases. Constructor/resource/services/project-command regressions60/60 pass; SDK10/11 pass. Parent independently reproduced `custom_provider_response_preserves_header_order_through_sdk_and_json` failing with `No API key found for ordered-custom` on unchanged49b0d18f and assigned the auth regression to233; not waived as a passing SDK suite.

At893a0d39: workspace/all-targets0 errors/0 warnings; scoped strict TS/Biome/fmt pass; inventoryPASS686/186679, statuses403/19/87/177 unchanged. Scoped lib+tests clippy exits0 with1679 warnings. One raw changed-function-span overlap is the unchanged `supervisor/main.rs` test-module placement (`items_after_test_module`); parent confirmed preexistence and reserved it for final clippy. No applicable new finding. Manual Rust/TS readability audited changed lines. Proof ledger and exact commands: `/tmp/pi-rust-repin-lossless-interactive/resource-loader-constructor-proof-ledger.md`. This is bounded constructor/error-carrier evidence, not whole-loader or whole-SDK acceptance. Source/native HOME initialization stays outside the armed failure; no host NSS mutation.
