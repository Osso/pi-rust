# footer-data-provider

## Native representation checkpoint

GitPaths/cwd/branch cache, GitBranchRunner arguments/results, executable resolution, and FooterSessionOverride cwd/thinking text are `JsString`. File reads, metadata/watch paths and child-process cwd encode only at actual IO. HEAD/stdout decode with Node replacement and use JS trim, retaining U+0085. Path traversal/resolution uses native Node lexical helpers, not PathBuf authority. Public constructors/set_cwd accept native text.

`raw_footer_statuses` covers high/low/pair repo paths and executable names, exact branch U+0085, plus frozen branch/executable output. Existing legacy provider test fixtures still need native API adaptation. Raw reftable runner/watcher/cancellation proof remains pending; no whole provider acceptance is claimed.

Source: `packages/coding-agent/src/core/footer-data-provider.ts` (450 lines).
Rust: `crates/pi-coding-agent/src/core/footer_data_provider.rs`.
Tests: `crates/pi-coding-agent/tests/footer_data_provider.rs` (ports
`test/footer-data-provider.test.ts`).

## Exports and branches

| TS export / branch | Rust item | Test |
|---|---|---|
| `findGitPaths` regular `.git` directory | `find_git_paths` dir branch | `find_git_paths_reads_regular_and_worktree_layouts`, `uses_head_directly_in_a_regular_repo_from_a_nested_directory` |
| `findGitPaths` `.git` file + `gitdir:` + missing HEAD | `find_git_paths` file branch | `find_git_paths_reads_regular_and_worktree_layouts` (worktree), missing-HEAD returns `None` (same branch) |
| `findGitPaths` `commondir` resolution | `find_git_paths` + `normalize_lexical` | `find_git_paths_reads_regular_and_worktree_layouts` |
| `findGitPaths` no repo / walk to root | loop with `parent()?` | covered by `set_cwd` target resolution; non-repo returns `None` |
| `resolveFooterExecutableName` default `pi` omitted | `normalize_footer_executable_name` | `footer_executable_name_omits_the_default_pi` |
| `resolveFooterExecutableName` `pi-*` accepted | `normalize_footer_executable_name` | same |
| `resolveFooterExecutableName` `PI_EXECUTABLE_NAME` wins | `resolve_footer_executable_name_with_env` | `footer_executable_name_prefers_the_wrapper_value` |
| `FooterDataProvider.getGitBranch` cached sync read | `FooterDataProvider::get_git_branch` | `uses_head_directly_in_a_regular_repo_from_a_nested_directory` |
| `.invalid` HEAD → `git symbolic-ref` → branch or `detached` | `GitBranchRunner::resolve_sync` | `resolves_the_branch_via_git_when_head_is_invalid` |
| non-`ref: refs/heads/` HEAD → `detached` | `resolve_branch_sync` | same |
| `getExecutableName` | `get_executable_name` | `footer_executable_name_*` (helper); process-env path is thin |
| `getExtensionStatuses` / `setExtensionStatus` / `clearExtensionStatuses` | `get_extension_statuses`, `set_extension_status`, `clear_extension_statuses` | `tracks_extension_statuses_and_provider_count` |
| `onBranchChange` returns unsubscribe | `on_branch_change` + `BranchChangeSubscription::drop` | `set_cwd_recomputes_the_branch_and_notifies` |
| `get/set/clearSessionOverride` | same names | compile-level; `FooterSessionOverride.session_manager` is type-erased (gap below) |
| `get/setAvailableProviderCount` | `get_available_provider_count`, `set_available_provider_count` | `tracks_extension_statuses_and_provider_count` |
| `setCwd` resets cache, watchers, notifies | `set_cwd` | `set_cwd_recomputes_the_branch_and_notifies` |
| `dispose` | `dispose` | exercised by every test teardown |
| `scheduleRefresh` 500 ms debounce + pending/in-flight | `schedule_refresh`, `refresh_git_branch_async` | `debounces_rapid_reftable_updates_into_a_single_async_refresh` |
| async refresh notifies only on branch change | `refresh_git_branch_async` | `does_not_notify_listeners_when_reftable_updates_keep_the_same_branch`, `updates_the_cached_branch_when_the_reftable_directory_changes` |
| `setupGitWatcher` HEAD directory watcher | `setup_git_watcher` + `utils::fs_watch` | `resolves_the_branch_via_git_in_a_reftable_worktree` (setup), refresh tests above |
| reftable directory + `tables.list` watchers | `setup_git_watcher` | `updates_the_cached_branch_when_the_reftable_directory_changes` |
| 1 s `tables.list` / WSL HEAD polling | poll task in `setup_git_watcher` | `debounces_*` (poll does not add refreshes inside the window); WSL path branch is unit-covered by `should_poll_git_head` |
| watcher error → clear + retry after 5 s | `handle_git_watcher_error`, `schedule_git_watcher_retry` | gap: triggering a real inotify error deterministically is not practical; retry constant asserted by `tests/fs_watch.rs` |
| `ReadonlyFooterDataProvider` projection | read-only methods on `FooterDataProvider` | — |

## Gaps / deviations

- `FooterSessionOverride.session_manager` is `Arc<dyn Any + Send + Sync>` until the
  session-manager slice lands `ReadonlySessionManager`; the footer never reads it.
  `ContextUsage` is defined locally with the same three fields because
  `core/extensions/types.ts` is not ported yet.
- The watcher-error retry path is implemented but not behaviorally triggered in
  tests (no deterministic inotify failure injection).
- `fs.watch` does not report reads; `utils::fs_watch` now drops `EventKind::Access`
  events, which also prevents HEAD reads from re-triggering the debounce loop.
  This matches Node's `fs.watch` observable behavior.

## Discrepancies

The extension-status Map-order discrepancy below was found and corrected during native migration.

## Native extension statuses

Status setters/getter/storage use `JsString`; getter returns `IndexMap<JsString,JsString>` to preserve frozen `Map` insertion order. Raw status replacement/removal/clear and exact entry order are compared with actual frozen FooterDataProvider in `raw_footer_statuses` (non-repository temp directory; no git watcher toggled). Existing BTreeMap sorted `a` before previously inserted `z`, unlike source (behavioral RED); native insertion-order storage corrects that Rust discrepancy. Git/session-override surfaces are unchanged by this slice.


## Callable status prerequisite (2026-10-24)

Frozen :25,175–183,208–214 requires Text/Callback carriers, lazy per-read evaluation with the current displayed override, omission without deletion and original Result rejection. The earlier text-only mapping is superseded for this surface. [Contract](../../specs/extension-status.md). Direct public API compile-RED is approved by parent for this missing typed API; source scheduled-render original-TypeError proof is GREEN in status-callback-source-throw.log. Native/differential checkpoint proof pending.
