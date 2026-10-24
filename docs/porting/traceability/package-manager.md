# coding-agent package-manager.ts traceability

Source: `packages/coding-agent/src/core/package-manager.ts` (2611 lines, pinned
revision in `docs/porting/source-lock.json`). Rust:
`crates/pi-coding-agent/src/core/package_manager.rs`.

Dependencies are reused, not duplicated: `utils::git::parse_git_url`,
`utils::paths::{resolve_path, relative_path, canonicalize_path, is_local_path,
mark_path_ignored_by_cloud_sync}`, `core::pi_manifest::read_pi_manifest`,
`core::output_guard::is_stdout_taken_over`, `utils::js_json`,
`pi_js_compat::minimatch`, the `ignore` crate (same matcher as the skill loader),
`semver`, `globset`, `sha2`, and `futures-util`.

## Approved directory-enumeration order gap

Frozen package-manager.ts:362–388 consumes readdirSync order when collecting skill paths; :2496 sorts resource precedence only, not sibling names. Native read_dir_entries preserves native filesystem order. Under the2026-10-23 no-libuv-oddities decision, equal-precedence sibling order may differ. SDK04 compares complete discovered skill records order-insensitively, still requires inline-last/filter behavior, and compares untouched console output with identical explicit input order. No directory sort or libuv emulation was added.

## Current whole-module re-port (partial)

| Surface | Current proof / remaining work |
|---|---|
| npm uninstall argv (:1764–1774) | Genuine current RED: extra --legacy-peer-deps; removed only uninstall flag, install flags retained. Full fake-binary transcript GREEN. |
| dedupe (:1658–1679) | Genuine same-scope project RED: last filter won. Native now retains first; only later project replaces existing user. Full metadata/enabled-state comparison GREEN. |
| source-absent autoload/delta architecture | Deleted delta base lookup, delta filters and all autoload reads from this module. Genuine actual-source RED compared global versus project npm storage; now uses project storage and ordinary filters exactly. Retired exclusive old-only unit `autoload_disabled_project_entry_is_a_delta_over_the_global_package`; ordinary filter/precedence tests remain. |
| command help | Stale assertion replaced with frozen package-manager-cli.ts:135 `Update pi and installed packages.`; execution help test GREEN. No production help change. |
| remaining whole-module work | Exact Result/error propagation and fallible progress callbacks approved; manifest/discovery/semver/process/env/timeout branches still under audit. No whole-module completion claim. |

Current actual-source differential GREEN14 (`q15b-package-logic-green.log`); help GREEN1 (`q15b-package-help-green.log`). Source assertions remain complete and unchanged except added first-wins/autoload cases. No new sort or directory-order normalization.

## Integration seam

`DefaultPackageManager` takes `Arc<dyn PackageSettingsStore>` instead of the
TypeScript `SettingsManager`. The trait exposes exactly the members this module
uses: `getGlobalSettings`/`getProjectSettings` (packages, extensions, skills,
prompts, themes), `setPackages`, `setProjectPackages`, `isProjectTrusted`, and
`getNpmCommand`. `PackageSource`/`PackageFilter` are re-exported from the ported
`core::settings_manager` (single definition; `source()`/`filter()` accessors live
there). `SettingsManagerStore` adapts the real `SettingsManager` through a mutex
so the package manager can keep `&self` methods; the settings module's
`SettingsStorage` gained a `Send` bound for that adapter. The trait remains so
tests can use in-memory fakes.

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
| same-scope first wins; project-over-global dedupe; source-absent autoload property ignored | `differential_same_scope_package_duplicates_keep_the_first_filter`, `differential_autoload_property_is_ignored_and_project_storage_owns_the_package`, retained project-over-global regression |
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
  matches keep filesystem order, and a leading `./` in a manifest entry is normalized like `glob` does. Directory walks use `read_dir` order, the
  same getdents order Node sees for the same directory.
- `minimatch` comes from `pi_js_compat` with default options, matching the
  TypeScript call sites.
- `semver`: exact, caret, tilde, wildcard, comparator, hyphen, and `||` union
  ranges are normalized to node-semver `validRange` text and matched with the
  `semver` crate; unsupported forms yield `range: None` (treated as unpinned).
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

- `cargo test -p pi-coding-agent --test package_manager` (30 tests)
- `cargo test -p pi-coding-agent --test package_manager_git` (7 passed, 2 ignored probes)
- `cargo test -p pi-coding-agent --test differential_package_manager` (12 tests)
- `cargo clippy -p pi-coding-agent --all-targets -- -D warnings`

## Spawn failures

Node reports `spawn <cmd> <ERRNO>` / `spawnSync <cmd> <ERRNO>`. Rust uses
`utils::child_process::node_spawn_error_message` with the Linux errno-name table;
`missing_npm_binary_reports_the_node_spawn_error_text` and
`non_executable_npm_binary_reports_the_node_spawn_error_text` assert `ENOENT` and `EACCES`.

## Q18 native package manager continuation (2026-10-22)

Resource paths and PathMetadata source/baseDir, configured source identities, npm
spec/name/version/range, progress/errors, package update labels, command argv/cwd,
and package JSON use the single JsString/JsValue authorities. Native node path
operations retain units; only concrete filesystem/process/crypto byte bindings
encode. Node crypto hash update encodes the native prefix/suffix once, as Node's
Hash.update(string) does. Process output/fs bytes decode at ingress.

Patterns use native Ignore/minimatch/JsRegex; the manifest glob walk now matches
with native minimatch rather than scalar globset. Dot-skipping traversal remains
unchanged. No scalar projection of arbitrary path patterns is retained.

### Grammar-only narrowing

`node_version_range`, `installed_version_satisfies`, and `parse_native_semver`
check UTF-8 conversion only at semver validation. Unmatched units cannot match the
ASCII semver grammar, so failure means invalid/no match; source JsString identity
continues unchanged. No checked conversion at other intermediate text APIs.
`max_satisfying`/`highest_version` retain original native version strings while
comparing parsed Version values. npm spec parsing uses native JsRegex, not a
scalar name projection.

### Proof status

`lossless_package_manager` covers raw local source/npm version parsing and a raw
extension directory's path/baseDir through resource resolution. Written but
unexecuted while coding compilation is blocked. replacement-packages-1:141 local
primaries; replacement-packages-2:12, subsequently addressed except queued parent
Settings native authority and spawn-error helper dependency. No runtime PASS or
whole-module frozen parity claim until integration compiles and fixtures execute.

Integration diagnostic `replacement-cli-2` after native Settings refresh: zero
package-manager primaries. Native snapshot deserialization uses `js_json::from_value`
directly; source comparison and optional upstream branch defaults corrected.
Library-wide compilation still blocked, so no tests executed or runtime PASS.

## Homedir Result propagation (2026-10-23)

`home_dir` uses shared node_os::homedir and returns PackageManagerError retaining the native SystemError source. Constructor, resolve_path/resolve_input, and add_auto_discovered_resources propagate errors; constructor original-path and ancestor-directory empty-string fallbacks are removed. Direct constructor call sites (resource loader, startup UI, package-command effects) consume Results; command listing is now fallible too.

`differential_package_manager_home` compares frozen/native constructor failure plus delayed NSS failure after successful construction in `resolve` (auto-discovery/conflict-resolution) and local `install` (resolvePath wrapper). A test-only preload redirects successful NSS HOME to a temporary directory and fails only after a marker is created; no live credentials or network. Exact EIO class/message matches in all three cases. RED/GREEN logs: `/tmp/pi-rust-repin-lossless-interactive/package-manager-home-{red,green}.log`.

## Historical common-pin ordering failures (approved gap)

The first three rows below are explicitly classified under the approved readdir-order policy, not remaining source-logic requirements. `differential_resolve_from_nested_project_directory` compares codex root markdown versus nested skill siblings; `differential_resolve_auto_discovery_and_skills` and `differential_resolve_autoload_delta_and_dedupe` compare keep/skip prompt siblings. These are equal-precedence raw enumeration order, not the frozen precedence sort. All three currently pass without changing assertions; that is not a portable sibling-order guarantee. npm argv drift is fixed by the current checkpoint above.

### Earlier diagnostic evidence

All four failures below reproduce identically at pre-Result `a4e8fcb7` and post-Result `c669a154` (actual and expected outputs equal after normalizing temporary fixture roots). Baseline `/tmp/pi-rust-repin-lossless-interactive/package-manager-baseline-a4e8fcb7.log`; current `package-manager-home-tests.log`. They remain open for the whole-module re-port, not approved deviations.

| Open test | Exact observed mismatch | Frozen authority |
|---|---|---|
| `differential_resolve_from_nested_project_directory` | native `.codex/skills/root.md` precedes `codex-skill/SKILL.md`; source reverses them | `collectSkillEntries` :348–414, readdir/iteration :362; tool skill collection :2357 |
| `differential_resolve_auto_discovery_and_skills` | native project prompts `skip.md`, `keep.md`; source `keep.md`, `skip.md` | `collectAutoPromptEntries` :459–489, readdir/iteration :467; project prompts :2364–2373 |
| `differential_resolve_autoload_delta_and_dedupe` | same project prompt order mismatch (`skip.md` before `keep.md` natively) | `collectAutoPromptEntries` :459–489 / :467; project prompts :2364–2373 |
| `differential_install_and_remove_npm_with_fake_binary` | native npm uninstall appends `--legacy-peer-deps`; source omits it | `uninstallNpm` :1764–1773 (npm argv :1773); install flag is separate at :1755 |

Native focused proof after Result migration: package_manager 32/32 and lossless_package_manager 2/2 GREEN. Package-command execution 27/28; `help_and_validation_stay_synchronous` has a separate stale help-description assertion (line 645), not changed here. Logs `package-manager-behavior-tests.log` and `package-manager-native-tests.log`.
