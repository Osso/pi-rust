# coding-agent package-manager.ts traceability

Source: `packages/coding-agent/src/core/package-manager.ts` (2699 lines, pinned
revision in `docs/porting/source-lock.json`). Rust:
`crates/pi-coding-agent/src/core/package_manager.rs`.

Dependencies are reused, not duplicated: `utils::git::parse_git_url`,
`utils::paths::{resolve_path, relative_path, canonicalize_path, is_local_path,
mark_path_ignored_by_cloud_sync}`, `core::pi_manifest::read_pi_manifest`,
`core::output_guard::is_stdout_taken_over`, `utils::js_json`,
`pi_js_compat::minimatch`, the `ignore` crate (same matcher as the skill loader),
`semver`, `globset`, `sha2`, and `futures-util`.

## Integration seam

`DefaultPackageManager` takes `Arc<dyn PackageSettingsStore>` instead of the
TypeScript `SettingsManager`. The trait exposes exactly the members this module
uses: `getGlobalSettings`/`getProjectSettings` (packages, extensions, skills,
prompts, themes), `setPackages`, `setProjectPackages`, `isProjectTrusted`, and
`getNpmCommand`. `PackageSource`/`PackageFilter` are the settings wire shapes and
must be shared with the settings-manager port; they are defined here because
`package-manager.ts` owns their behaviour.

## Exported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `PathMetadata` | `PathMetadata` | differential `resolve` (all scenarios) |
| `ResolvedResource` | `ResolvedResource` | differential `resolve` |
| `ResolvedPaths` | `ResolvedPaths` | differential `resolve` |
| `MissingSourceAction` | `MissingSourceAction` | `resolve` skip/error/install branches (offline probe, install differential) |
| `ProgressEvent` / `ProgressCallback` | `ProgressEvent`, `ProgressType`, `ProgressAction`, `ProgressCallback` | `installs_npm_packages_with_the_managed_argv_and_reports_progress`, install differential |
| `PackageUpdate` | `PackageUpdate`, `PackageUpdateType` | `check_for_available_updates_reports_unpinned_installed_packages` |
| `ConfiguredPackage` | `ConfiguredPackage` | `lists_configured_packages_with_installed_paths`, differential |
| `PackageManager` interface | inherent `DefaultPackageManager` methods | compile-time call sites |
| `DefaultPackageManager` constructor | `DefaultPackageManager::new` | every test |
| `setProgressCallback` | `set_progress_callback` | install test |
| `addSourceToSettings` | `add_source_to_settings` | `add_and_remove_sources_normalizes_local_paths`, `replaces_a_filtered_source_while_keeping_its_filters`, differential |
| `removeSourceFromSettings` | `remove_source_from_settings` | same |
| `getInstalledPath` | `get_installed_path` | `installed_npm_packages_resolve_from_the_managed_root`, differential |
| `resolve` | `resolve` | 10 resolve tests + 6 differential resolve scenarios |
| `resolveExtensionSources` | `resolve_extension_sources` | `resolve_extension_sources_uses_temporary_and_project_scopes` |
| `listConfiguredPackages` | `list_configured_packages` | list test, differential |
| `install` | `install` | npm/bun/pnpm/local/untrusted tests, install differential |
| `installAndPersist` | `install_and_persist` | exercised through settings add test path |
| `remove` | `remove` | npm/local/remove differential |
| `removeAndPersist` | `remove_and_persist` | compile-time path (settings remove test) |
| `update` | `update` | unpinned/pinned test, git update tests, missing-source suggestions |
| `checkForAvailableUpdates` | `check_for_available_updates` | update-availability test |
| `getExtensionTempFolder` | `get_extension_temp_folder` | temporary git/npm path tests |

## Branches

| Branch | Test |
| --- | --- |
| npm scoped/unscoped specs, exact/range/`latest` versions, invalid range (`example?`) | `parses_npm_sources`, differential `parse_source` |
| git HTTPS/SSH/scp/shorthand/ref/`.git`, local fallback | `parses_git_and_local_sources`, differential `parse_source`/`identity` |
| package identity ignores version/ref and normalizes SSH/HTTPS | differential `identity`, dedupe test |
| project-over-global dedupe; `autoload:false` delta kept | `dedupes_...`, `autoload_disabled_...`, differential |
| resource precedence (project local > project auto > user local > user auto > package) | differential `resolve` order assertions |
| include/exclude/`+`/`-` patterns, exact-path matching, SKILL.md parent matching | pattern tests, differential |
| manifest globs and manifest-level overrides | `manifest_globs_...`, `package_filters_...`, differential |
| extension discovery (top-level files, subdir `index.ts`/manifest only) | multi-file test |
| skill discovery (pi root markdown, `SKILL.md`, `.agents`, `.codex`, `.claude`, ancestors, user vs project) | `discovers_ancestor_agents_and_tool_skill_directories`, differential auto scenario |
| `.gitignore`/`.ignore`/`.fdignore` per directory, prefixed rules, negation | gitignore test |
| project trust gates project installs and project auto-discovery | `untrusted_project_*`, differential untrusted |
| npm managed root/legacy pnpm/global root resolution, temporary hash dirs | installed-package test, temporary scope test |
| npm argv for npm/bun/pnpm, `npmCommand` prefixes, `--omit=dev` | bun/pnpm test, prefix test, probe tests |
| npm view JSON string/array, `maxSatisfying`, `rcompare`, errors | update tests |
| git clone/checkout, dependency install, marker file, reset+clean, pinned refs, prune | `package_manager_git.rs` tests |
| offline mode (`PI_OFFLINE`) skips installs/updates | offline probe test |
| command capture timeout/exit/signal formatting | exercised by git/npm failure paths; timeout branch covered by source inspection only (no test injects a hanging child) |

## Deviations and gaps

- `getEnv()` is not copied into the child environment: Rust children inherit the
  process environment (same variable set on every supported platform) and merge
  the extra `GIT_TERMINAL_PROMPT=0`.
- `stdio` takeover: when `is_stdout_taken_over()` is true, stdout is redirected to
  `/dev/stderr` (Unix) instead of Node's duplicated fd 2; on non-Unix it inherits
  stdout. Spawn failures report `"{command} {args} failed: {error}"` instead of
  Node's `spawn ... ENOENT` message.
- `glob` is replaced by a `globset` walk over the package root. Dot entries are
  skipped globally (Node's `dot:false` only excludes them for wildcard segments),
  and matches keep filesystem order.
- `minimatch` comes from `pi_js_compat` with default options, matching the
  TypeScript call sites.
- `semver`: exact, caret, tilde, wildcard, comparator, and `*` ranges are
  normalized to node-semver text; hyphen ranges and `||` unions are unsupported
  (`range` becomes `None`, so the package is treated as unpinned).
- Timeout branch of `runCommandCapture` has no automated test (would require a
  deliberately hanging child); the code path mirrors the source.
- Clone-failure cleanup has no test: exercising it requires a network clone or a
  `file://` source, which `parseSource` classifies as local.

## Discrepancies (source behaviour preserved)

- `resolvePath` errors propagate from `resolveLocalEntries`, matching the
  TypeScript throw.
- `findSuggestedConfiguredSource` returns the first suggestion in settings order.
- `getPackageManagerName` returns `""` when `npmCommand` ends with `--`.

## Proof

- `cargo test -p pi-coding-agent --test package_manager` (29 tests)
- `cargo test -p pi-coding-agent --test package_manager_git` (7 passed, 2 ignored probes)
- `cargo test -p pi-coding-agent --test differential_package_manager` (12 tests)
- `cargo clippy -p pi-coding-agent --all-targets -- -D warnings`
