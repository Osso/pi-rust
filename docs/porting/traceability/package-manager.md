# coding-agent package-manager.ts traceability

Source: `packages/coding-agent/src/core/package-manager.ts` (2611 lines, pinned
revision in `docs/porting/source-lock.json`). Rust:
`crates/pi-coding-agent-packages/src/core/package_manager.rs`.

Dependencies are reused, not duplicated: `utils::git::parse_git_url`,
`utils::paths::{resolve_path, relative_path, canonicalize_path, is_local_path,
mark_path_ignored_by_cloud_sync}`, `core::pi_manifest::read_pi_manifest`,
`core::output_guard::is_stdout_taken_over`, `utils::js_json`,
`pi_js_compat::{minimatch,node_ignore}`, the ported [node-semver dependency](package-manager-semver.md), `sha2`, and `futures-util`.

## Exact discovery order (2026-10-25)

Parent explicitly retired the former directory-order gap for ordinary discovery. Node readdir's libuv scandir orders raw name bytes; native read_dir_entries now sorts those bytes before any decoding, filtering, recursion or precedence sort. All discovery consumers use this helper; the other direct read_dir only checks whether a directory is empty, so ordering cannot change its result. No discovery algorithm or error handling changed.

Actual frozen/native trust-bootstrap RED `/tmp/ext-api3-resource-trust.log` loads failed→global versus native global→failed, changing factory side effects. The real readdir fixture retains every order assertion, with no manifest or explicit-path workaround. Loader and resource-theme discovery plus locale-sort ties receive the same correction and behavioral proofs. Current proof: [extension API closure](../ext-api-repin-proof.md). Earlier order-insensitive SDK/glob comparisons remain historical bounded evidence, not fresh order proof.

## Current whole-module re-port

| Surface | Current proof / remaining work |
|---|---|
| npm uninstall argv (:1764–1774) | Genuine current RED: extra --legacy-peer-deps; removed only uninstall flag, install flags retained. Full fake-binary transcript GREEN. |
| dedupe (:1658–1679) | Genuine same-scope project RED: last filter won. Native now retains first; only later project replaces existing user. Full metadata/enabled-state comparison GREEN. |
| source-absent autoload/delta architecture | Deleted delta base lookup, delta filters and all autoload reads from this module. Genuine actual-source RED compared global versus project npm storage; now uses project storage and ordinary filters exactly. Retired exclusive old-only unit `autoload_disabled_project_entry_is_a_delta_over_the_global_package`; ordinary filter/precedence tests remain. |
| command help | Stale assertion replaced with frozen package-manager-cli.ts:135 `Update pi and installed packages.`; execution help test GREEN. No production help change. |
| error/callback boundaries (:791–877,944–970,1153–1216,1278–1307,1380–1395,1638–1652) | Boxed shared CallbackError retains raw/native throws and diagnostic sources. Getter/list/identity/match/dedupe/local-resolution/update-check failures propagate Results instead of .ok/fallback drops. Nine real delayed-NSS child-process cases and25 actual-source progress cases GREEN. Source-catch String(value) delegates to the shared authority; callback reentrant clear remains safe. |
| semver dependency (:30,49–54,1429,1477) | Reduced parser deleted; one private native port of frozen node-semver7.8.0. Upstream323 strict/default fixture rows and actual-source55-case range/ten-scenario version corpus. [Source mapping/proof](package-manager-semver.md). |
| capture/sync output (:2544–2610) | Six actual-source REDs (BOM/NEL trim in both paths, split UTF-8, timeout after parent exit while descendants hold pipes). Eighteen real-process scenarios GREEN, including stderr selection, malformed bytes, signals and delayed EOF. JS trim, stateless per-read decoding, timeout through full close; no task-join/empty-output fallbacks. |
| gap: libuv read partitioning | Approved runtime-oddity policy: native OS read partitions may differ from Node/libuv. Each delivered native chunk still decodes independently; controlled split writes and malformed bytes have exact actual-source proof. |
| spawn string validation/native errors (:2515–2542,2596–2610) |26 actual-source RED observations;42 cases GREEN across runCommand/runCommandCapture/runCommandSync. Shared file/argv/cwd/env validators retain exact ordering, coded TypeError and UTF-16 inspection; shared IO records retain errno/code/syscall/path/spawnargs, including bare ENOTDIR. Sync preflight throws escape the result.error message wrapper. |
| environment/stdio (:6–23,2515–2535,2597) | Actual-source RED for all three paths ignoring the shared environment overlay and ordinary inherited stdin returning EOF. Environment replacement, capture override with a lone surrogate, empty values, Linux empty-env restoration, and both stdin/output takeover modes GREEN (`q15c-env-stdio-green-1.log`). Additional actual-source RED showed `/dev/stderr` reopened at offset zero; native now duplicates fd2. Prefixed regular-file stderr comparison GREEN (`q15c-sync-limit-fd-green-2.log`). |
| gap: restored env prototype names (:12–17) | Under the approved Node/V8 runtime-oddity policy, native restoration uses own string keys; JS `__proto__` assignment behavior on the temporary plain-object env is not reproduced. |
| gap: spawnSync implicit maxBuffer (:2599–2608) | Frozen supplies no explicit maxBuffer. Node's implicit1MiB cap is excluded by the approved 2026-10-23 runtime-oddity policy (parent reaffirmed 2026-10-24). Removed the cap emulation introduced in2939ca05 and its limit differential; native captures complete output. `q15c-sync-limit-fd-green-2.log` limit proof is retired, not parity evidence. The independent fd2 offset-sharing proof remains valid. |
| concurrency (:1083–1114,1214,1608–1629) | Real-process ordering RED: native waited for a gated survivor instead of rejecting. Native owned workers now reject immediately, stop only the failed worker, and detach surviving workers; callback/cache state remains shared. Exact rejection, pre-release start set, and post-rejection queued completion match frozen. GREEN `q15c-concurrency-green-2.log`. |
| gap: worker scheduling | Independent worker continuation/microtask interleavings are not emulated under the approved runtime-oddity policy. Worker bound, failed-worker termination, first rejection, survivor lifetime and ordered successful result values remain frozen logic. |
| checkpoint status | Whole-module row closed on59489965 after parentf9167bc9 merge. Public/package differential regressions56 GREEN; process group8 GREEN (two child-entry guards, six behavioral tests); current stdin fixture GREEN after lint-only test fix. Workspace/all-target check0 warnings/errors; changed-line all-target clippy0 findings; source inventory686/186679 PASS. Independent final milestone verification remains parent-owned. |

Current actual-source differential GREEN14 (`q15b-package-logic-green.log`); help GREEN1 (`q15b-package-help-green.log`). Source assertions remain complete and unchanged except added first-wins/autoload cases. No new sort or directory-order normalization.

Constructor/ancestor path continuation: frozen constructor783–785 resolves both inputs against process cwd independently. Genuine RED (`q15b-package-relative-cwd-red.log`) returned undefined for relative agent/pkg because native doubled agent/agent. Constructor now uses the shared process-cwd authority; isolated native/source child test GREEN. Ancestor collectors426–457 use shared Node path.resolve, not path-input normalization with an original-input fallback; shared traversal retains agents/codex/claude order. Existing14 source differential + home/relative grouped2 GREEN (`q15b-package-cwd-paths-green.log`).

Manifest-glob continuation: actual-source hidden path RED (`q15b-package-glob-red.log`) and non-leading globstar/symlink RED (`q15b-package-glob-expanded-1.log`) led to replacing the root-only/dot-skipping walk. Fourteen concrete patterns compare complete records within one equal-precedence package group (only raw FS enumeration ignored). GREEN `q15b-package-glob-expanded-2.log` covers explicit hidden, parent, absolute, globstar-parent, wildcard-dot exclusion, escaped star, duplicate/parent braces, trailing directory slash, hash/bang names and explicit/non-leading symbolic links. Native parser-backed literal accessor is parent-approved and independently tested; no duplicate escape logic.

OS filename decoding uses `pi_js_compat::node_fs::decode_node_path` at every DirEntry boundary in this module, including the glob walker; no local to_string_lossy conversions remain. `differential_glob_decodes_non_utf8_filenames_at_the_node_boundary` compares actual frozen output for a raw0xFF filename, a distinct U+FFFD filename and a normal control. Complete two-record output/dedupe is asserted (only sibling enumeration order ignored). The baseline already matched; this is authority consolidation, not a claimed behavioral RED.

### Process output REDs and proof

`q15b-package-process-red-2.log` captures six concrete mismatches against frozen private methods. `q15b-package-process-green-1.log` covers all18 cases through actual subprocesses, including split multibyte writes, invalid bytes, BOM versus NEL whitespace, stderr-only success, both error-detail branches, SIGTERM, direct timeout and descendant-held EOF with/without timeout. The descendant timeout fixture first asserts the frozen operation rejects, ensuring the intended process-lifetime boundary is exercised. No network or installed package tool is invoked. IO errors propagate; ignored read errors and task-join empty defaults were removed. Native/libuv chunk partition differences are the explicit runtime gap, not a claim of identical scheduling. Private-method oracle operations are test seams only.

Final proof: implementation `a0de9c8d`, oracle typing-only correction `912a4c50`, parent-approved test oracle consolidation `73ffaba7`. Current18-case process differential GREEN (`q15b-package-process-shared-oracle.log`); shared CLI date consumer GREEN (`q15b-package-date-shared-oracle-corrected.log`; earlier zero-test filter is not evidence). Package17-test differential +10-version scenario regression GREEN (`q15b-package-process-regression.log`); all-target check GREEN (`q15b-package-check-shared-oracle.log`). All-target warning-mode clippy JSON reaches689 target artifacts with zero errors/zero findings on owned changed lines (`q15b-package-clippy-shared-oracle-filtered.json`). Oracle tsgo and Biome lint GREEN after removing the stale unknown return annotation/dead Response alias (`q15b-package-process-oracle-{typecheck,lint}-green.log`). No suppressions. Unit process/date consumers share `crate::test_oracle`; other lane-owned include! sites are untouched.

### Spawn validation/error records

`q15b-package-spawn-red.log`:26 actual-source discrepancies across inherited/captured/sync launch paths. `q15b-package-spawn-green-1.log`:42 spawn cases plus the existing18 output cases GREEN. Corpus covers empty/NUL file, first NUL argv and128-unit inspected truncation with a surrogate, cwd before env validation, missing cwd/file, EACCES, bare ENOTDIR, raw-surrogate missing executable, and an actual U+FFFD executable reached through an unpaired input string at Node's UTF-8 OS boundary. Sync's ignored cwd/env options are controlled explicitly; its validation errors remain uncaught TypeErrors rather than being wrapped as result.error. Native env names remain String; values remain JsString until the OS boundary, under the established name exclusion. Error oracle comparison includes code/errno/syscall/path/spawnargs when actually present; source assertions were not weakened. [Contract](../../specs/package-manager-spawn.md).

Post-commit proof on `cb929a50` (implementation `ca1e4c57`, parent `8d20164a`): focused public npm install/remove +55-range +10-version regression GREEN (`q15b-package-spawn-regression.log`); package all-target check GREEN (`q15b-package-spawn-check.log`); all-target warning-mode clippy exits0 with zero errors/owned changed-line findings (`q15b-package-spawn-clippy-filtered.json`); oracle tsgo/Biome lint GREEN (`q15b-package-spawn-oracle-{typecheck,lint}.log`). Manual changed-line Rust/TS readability audit; no suppressions or reduced assertions.

### Error/callback REDs and proof

`q15b-package-home-getter-red.log`: frozen getInstalledPath threw the real delayed homedir SystemError; native returned undefined. Extended existing NSS preload harness now compares constructor/resolve/install/getInstalledPath/identity/list/remove/add-match/checkForAvailableUpdates; no live state or remote commands. `q15b-package-progress-red.log`: supplied-false start callback was ignored and native completed instead of rejecting false. Fallible callback propagation now matches start outside catch, complete inside catch, error callback replacement, failing raw ToString, native UTF-16 TypeError/cause/code and clearing the handler from itself. GREEN2 grouped tests (`q15b-package-carrier-green-1.log`), including all9/25 cases. Public successful-result consumers explicitly unwrap or propagate the new Results; CLI listing and progress adapters migrated without dropping errors.

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
| command capture timeout/exit/signal formatting | `capture_and_sync_output_boundaries_match_frozen`: actual sleeping child and parent-exit/descendant-held-pipe cases; complete error/output comparison |

## Deviations and gaps

- Environment forwarding uses shared `process_env`, with capture overrides and frozen Linux empty-env restoration. Native environment names retain the established scalar domain.
- `stdio` takeover duplicates fd2 on POSIX (including its shared offset) and ignores stdin; ordinary commands inherit all three descriptors. Non-POSIX behavior remains the approved platform gap. Async spawn failures use shared Node records with exact deferred executable/argv fields versus bare synchronous libuv syscall failures.
- Manifest glob traversal now consumes path segments with the shared minimatch literal/brace/matcher authority: explicit hidden/parent/absolute paths, globstar/parent, escapes, duplicate brace matches, directory-only suffixes and one-symlink globstar behavior match actual source. Filesystem enumeration now follows the raw-byte Node readdir order described above.
- `minimatch` comes from `pi_js_compat` with default options, matching the
  TypeScript call sites.
- Semver uses the native strict/default [node-semver7.8.0 port](package-manager-semver.md), not the Rust semver crate or a reduced supported-forms approximation.
- `runCommandCapture` timeouts are tested with a sleeping child and an exited parent whose descendant retains both pipes. Timer remains active until stream EOF, not merely child exit. On timeout, already-decoded chunks survive cancellation/re-entry and an already-reaped child's cached status is retained.
- Git mutation tests use local repositories; subprocess tests use executable fixtures. No network clone or live package tool is required for process parity proofs.

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
with native minimatch rather than scalar globset. Explicit hidden path segments are traversed; implicit wildcard dot exclusion follows the pinned parser. No scalar projection of arbitrary path patterns is retained.

### Grammar-only narrowing

Semver grammar matching and expansion use shared UTF-16 JsRegex/JsString throughout. Only regex-validated ASCII numeric captures narrow for shared Number conversion. Unmatched units remain in original version strings and native error messages. `max_satisfying`/`highest_version` retain original strings while comparing parsed private SemVer records. npm spec parsing also uses native JsRegex, not a scalar name projection.

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

The first three rows below historically used the readdir-order waiver; that waiver is superseded by the exact discovery-order correction above. `differential_resolve_from_nested_project_directory` compares codex root markdown versus nested skill siblings; `differential_resolve_auto_discovery_and_skills` and `differential_resolve_autoload_delta_and_dedupe` compare keep/skip prompt siblings. These are equal-precedence raw enumeration order, not the frozen precedence sort. All three currently pass without changing assertions; that is not a portable sibling-order guarantee. npm argv drift is fixed by the current checkpoint above.

### Earlier diagnostic evidence

All four failures below reproduce identically at pre-Result `a4e8fcb7` and post-Result `c669a154` (actual and expected outputs equal after normalizing temporary fixture roots). Baseline `/tmp/pi-rust-repin-lossless-interactive/package-manager-baseline-a4e8fcb7.log`; current `package-manager-home-tests.log`. They remain open for the whole-module re-port, not approved deviations.

| Open test | Exact observed mismatch | Frozen authority |
|---|---|---|
| `differential_resolve_from_nested_project_directory` | native `.codex/skills/root.md` precedes `codex-skill/SKILL.md`; source reverses them | `collectSkillEntries` :348–414, readdir/iteration :362; tool skill collection :2357 |
| `differential_resolve_auto_discovery_and_skills` | native project prompts `skip.md`, `keep.md`; source `keep.md`, `skip.md` | `collectAutoPromptEntries` :459–489, readdir/iteration :467; project prompts :2364–2373 |
| `differential_resolve_autoload_delta_and_dedupe` | same project prompt order mismatch (`skip.md` before `keep.md` natively) | `collectAutoPromptEntries` :459–489 / :467; project prompts :2364–2373 |
| `differential_install_and_remove_npm_with_fake_binary` | native npm uninstall appends `--legacy-peer-deps`; source omits it | `uninstallNpm` :1764–1773 (npm argv :1773); install flag is separate at :1755 |

Native focused proof after Result migration: package_manager 32/32 and lossless_package_manager 2/2 GREEN. Package-command execution 27/28; `help_and_validation_stay_synchronous` has a separate stale help-description assertion (line 645), not changed here. Logs `package-manager-behavior-tests.log` and `package-manager-native-tests.log`.
