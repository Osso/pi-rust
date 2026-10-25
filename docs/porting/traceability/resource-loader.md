# resource-loader.ts → core::resource_loader

Frozen `4f9e6f8a4`, 1191 lines. Native: `crates/pi-coding-agent-tools/src/core/resource_loader.rs`.
Whole-module audit closed 2026-10-25. Current commands, revisions and results: [extension API proof](../ext-api-repin-proof.md).

## Exported contracts

| Frozen symbol | Native mapping | Observable proof |
| --- | --- | --- |
| ResourceCollision / ResourceDiagnostic | diagnostics reexports | conflict/collision assertions in `resource_loader` |
| ResourceExtensionPaths | ResourceExtensionPaths / ExtendedResourcePath | extend-resource paths, metadata and invalid-path differential |
| ResourceLoaderReloadOptions | ResolveProjectTrust callback | trust bootstrap behavioral cases |
| ResourceLoader | ResourceLoader trait | default/static getters, reload and extend tests |
| loadProjectContextFiles | load_project_context_files | actual frozen 8-tree context differential, directory warnings and root-error differential |
| loadRulesFilesFromDir / loadRulesFromDir | same snake-case functions | actual frozen rule differential, locale sorting, empty files, EISDIR failure |
| RulesScope | RulesScope | shared/main/child/architect selection and trusted-project rules |
| DefaultResourceLoaderOptions / DefaultResourceLoader | same names | constructor boundary and 23 resource behavioral tests |
| InlineExtension | native factory/name representation | factory failures, disabled inline IDs, conflict and trust tests |

## Branch audit

| Frozen branch / private methods | Native mapping | Proof |
| --- | --- | --- |
| resolvePromptInput: absent/literal/file/read failure | resolve_prompt_input | literal/file tests; exact stderr FORCE_COLOR=0/1 differential |
| loadContextFilesFromDir / loadInstructionContextFilesFromDirs | same snake-case helpers | AGENTS family suppresses CLAUDE family globally; all uppercase/local variants; directories warn; no override-file branch |
| appendUniqueContextFiles / listAncestorDirs | same helpers | realpath dedupe, root-to-leaf order and memory symlink fixtures |
| global memory exclusion | load_project_context_files | global/project memory fixture trees |
| constructor normalization and dependency creation | fallible new | eight frozen/native constructor/caller identity cases; owning constructor proof linked below |
| getters | ResourceLoader trait methods | empty state, successful reload, retained partial state, native text tests |
| extendResources / normalizeExtensionPaths / mergePaths | extend_resources and helpers | metadata normalization, reentrant getters, canonical first-wins merge and path errors |
| loadProjectTrustExtensions / loadCurrentExtensionSet | same snake-case methods | forced-untrusted bootstrap, CLI/noExtensions and inline factory tests |
| reload package/CLI paths and enabled metadata | resolve_resource_sets | project-first collisions, no-resource switches and explicit additional paths |
| loadFinalExtensionSet / resolveExtensionLoadPath | same snake-case methods | pre-trust runtime reuse, failure-path reconciliation and disabled inline filtering |
| applyExtensionSourceInfo / findSourceInfoForPath / getDefaultSourceInfoForPath / isUnderPath | same helpers | package/extension prefix metadata, synthetic paths and ordinary local roots |
| mapSkillPath | map_skill_path | auto/package SKILL.md selection and explicit skill fixtures |
| updateSkillsFromPaths / updatePromptsFromPaths | same snake-case methods | overrides, source metadata and retained stage publication |
| updateThemesFromPaths | same method | valid additional-theme default source metadata differential; package/extension/existing/default precedence |
| loadThemes / loadThemesFromDir / loadThemeFromFile | same methods | file/directory/symlink theme discovery, warning/collision paths and overridden themes |
| dedupePrompts / dedupeThemes | same helpers | first-wins names and exact collision diagnostics |
| getDisabledExtensionNames / isExtensionPathDisabled / filterDisabledExtensionPaths / isExtensionFactoryDisabled | shared loader helpers plus resource-root resolution | trimmed IDs, basename/path/native-unit identity and disabled factories |
| loadExtensionFactories | same method | inline numbering, native Error messages and frozen non-Error fallback differential |
| detectExtensionConflicts / addExtensionConflictDiagnostics | same helpers | exact Tool/Flag diagnostics; keep both loaded |
| rule scope/trust selection | load_rules_into | shared and role scopes, untrusted/trusted project rules |
| discoverSystemPromptFile / discoverAppendSystemPromptFile | same helpers | trusted project precedence, global defaults and explicit literal/file sources |
| system/append prompt overrides | load_system_prompts_into | actual frozen two-reload publication differential |

## Corrected discrepancies

The successor audit reproduced the remaining realistic defects before code changes:

- Append prompt callbacks saw the previous system prompt. RED `/tmp/ext-api3-resource-prompt-red.log`; publish the existing state before append source/override processing, with no callback under a lock.
- Direct additional themes had no default source metadata. RED `/tmp/ext-api3-resource-theme-red.log`; preserve source precedence and fill the missing local default branch.
- Directory traversal used filesystem order instead of Node readdir's raw-byte name order. RED `/tmp/ext-api3-theme-order-red2.log` proves visible theme order; `/tmp/ext-api3-rule-order-red.log` proves stable locale ties for composed/decomposed filenames. The real trust-bootstrap differential also proves single loading of successful and failed pre-trust factories; its package-manager helper correction is parent-approved.

Earlier fixes are renewed by the current focused tests: extension/skill/prompt/theme/context stage publication; rule-read failure retaining earlier changes; extend-resource callback deadlock; path/metadata errors; colored Node warning text; directory candidates; old-only prompt-source getters and AGENTS.override/worktree-shadow behavior removed. No old-only behavior remains in this module.

Current focused results for `lossless_resource_loader`, 23 `resource_loader` behaviors and two frozen rule/context parents are recorded in the closure proof. The single ignored warning probe is invoked by its parent, not skipped coverage.

## Representation and gaps

- Rust-native extensions and static factory registration replace jiti loading/caching, as approved in AGENTS.md. Getters return owned snapshots; live extension handles remain shared where frozen consumers require them.
- Typed native Skill/PromptTemplate values always carry source info; erased JS optional/union aliases need no additional native carrier.
- Gap: adversarial shapes, exceptional filesystem failures/races, runtime oddities and numeric extremes are outside the current parity bar; ordinary read/path diagnostics remain exact.

Constructor error identity contract: [spec](../../specs/resource-loader-constructor-errors.md). Existing constructor proof is `/tmp/pi-rust-repin-lossless-interactive/resource-loader-constructor-proof-ledger.md`; this closure does not rerun the fault-injection matrix.
