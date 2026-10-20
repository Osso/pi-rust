# Extension examples E

Opt-in native embedding, no default loading. Pinned source:
`532f7d12`, `packages/coding-agent/examples/extensions/`.
Each test includes actual example source with `#[path]`, explicitly registers
its factory, and uses the real Rust loader/runner and UI. Shared fixture
`tests/support/extension_examples_e.rs` reuses `extensions.rs` and `oracle.rs`,
extending the A/B transcript pattern. `oracle/extension-examples-e-oracle.ts`
loads unchanged pinned factories using the real TS loader/runner, in-memory
session manager and faux model registry. No providers or live state.

Tests compare entire runner results, UI traces, commands, errors (excluding
stack frames), and action traces. Temp-root prefixes alone are normalized.
Resource handler results include every path and extension provenance.

| TS source / branches | Native example | Behavioral proof |
|---|---|---|
| `prompt-customizer.ts:24–70` selected tools, missing/empty tools, skill names/order; `74–97` missing/empty/nonempty user append, factory | `examples/extensions/prompt_customizer.rs` | `example_prompt_customizer::complete_prompt_results_match_all_tool_skill_and_append_branches`: 43 real-runner differential cases; includes complete messages/systemPrompt result |
| `claude-rules.ts:31–54` missing directory, recursive traversal, sorted file order, file-only lowercase `.md`; `57–86` scan/update/cache, notification, empty rules, prompt append | `examples/extensions/claude_rules.rs` | `example_claude_rules::rules_are_scanned_recursively_sorted_and_refreshed_only_at_session_start`; `scandir_errors_are_reported_with_node_messages` compares full Node ENOTDIR error |
| `project-trust.ts:17–64` per-load count, all five choices, cancellation, empty/nonempty/cancelled note, no UI, modes, session-start handler | `examples/extensions/project_trust.rs` | `example_project_trust::all_choices_notes_cancellation_and_modes_match_complete_trust_results`: 28 steps, complete trust decision/remember and UI trace |
| `dynamic-resources/index.ts:1–15` module-relative skill/prompt/theme paths, discovery on startup/reload | `examples/extensions/dynamic-resources/index.rs` | `example_dynamic_resources::startup_and_reload_return_complete_paths_with_real_assets`: full returned paths/provenance and byte-identical companion assets |
| `trigger-compact.ts:3–50` unknown tokens, initial sample, threshold equality, upward crossing, repeated high usage, retained prior sample across unknown usage, command trim/default, UI/no UI, completion/error callbacks | `examples/extensions/trigger_compact.rs` | `example_trigger_compact::threshold_crossings_unknown_usage_commands_and_callbacks_match`: entire compaction arguments and UI callback traces, both outcomes |
| `auto-commit-on-exit.ts:9–49` status failure/clean, reverse assistant scan, text-block filtering/joining, first-line fallback, 50 UTF-16 units and ellipsis, stage all, commit success/failure, UI/no UI | `examples/extensions/auto_commit_on_exit.rs` | `example_auto_commit_on_exit::shutdown_commits_all_changes_using_latest_assistant_first_line`; `no_repo_clean_repo_no_assistant_no_ui_and_commit_failure_match`: actual temp git commits, full messages/status/files and UI; split-surrogate argv bytes and terminal text |
| `git-checkpoint.ts:10–53` no leaf, current leaf, no ref/entry, stash creation, matching historical entry, absent/UI/cancel/no/Yes-prefix, restore success/failure, map clear with retained current id | `examples/extensions/git_checkpoint.rs` | `example_git_checkpoint::checkpoints_restore_only_on_matching_entry_and_yes_with_ui`; `clean_stash_missing_repo_agent_end_clear_and_failed_apply_preserve_source_behavior`: real stash/create/apply, full file/status/commit state and UI/results |

## Representation and discrepancies

- Dynamic resources: host supplies the resource directory explicitly, the native
  equivalent of `dirname(import.meta.url)`; never use session cwd. Three original
  resource assets accompany the Rust example unchanged.
- JS invalid runtime types remain outside typed Rust API contracts.
- Claude rules preserves source behavior: symlink entries are not traversed;
  failed refresh leaves prior cached files. Existing non-directory roots report
  the Node `scandir` error. No speculative filesystem recovery.
- Auto-commit UTF-16 slicing can create a lone surrogate from valid text (49 ASCII
  units followed by an astral character). Git argv uses U+FFFD exactly as Node.
  `String`-typed native UI also uses U+FFFD, matching terminal bytes, not JS's
  unrepresentable in-memory unit. Classification: existing typed-boundary
  exclusion, [compatibility](../../specs/compatibility.md#out-of-scope), line 177
  (related UTF-16 split exclusion at line 194). The TS fixture converts notify
  text to UTF-8 bytes and back only for this terminal boundary; all normal text
  is unchanged. A concrete split-boundary test checks commit bytes and notification.
- Git-checkpoint source discrepancy: nonzero `stash apply` still announces
  success. Classified as source behavior preserved under the explicit values /
  order / errors requirement; test proves conflicting edits remain unchanged
  despite the notification. `agent_end` clears refs but retains the current id;
  subsequent turns deliberately reuse that id, also tested.
- Git tests relaunch themselves inside the Cargo runner's private-state guard,
  with system/global git config disabled and all repos in fresh temp dirs.
  Commit rejection uses a private executable `false` pre-commit hook. No live
  repos, config, credentials, databases, providers or direct oracle execution.

## Blocked / reassigned

`dynamic-tools.ts` needs **late registration visible to an already-created
runner**. `ExtensionApi::register_tool` exists (`types.rs:3047–3061`) but updates
an `Arc<Mutex<Extension>>`; `load_extension_from_factory` returns a clone snapshot
(`loader.rs:200–204`). `ExtensionRunner::get_all_registered_tools` /
`get_tool_definition` read snapshot maps (`runner.rs:516–534`), so a tool
registered by a real command/session handler is invisible. Missing-module RED
and behavioral failure are in the ledger; draft source/tests were removed,
not stubbed or marked done. Main assigned a separate core fix and dynamic-tools
port (`fix/extension-runtime-registration`); no core files changed here.

## Proof ledger

`/tmp/pi-rust-ext-examples-e/ledger.md` records exact targeted commands and logs.
Every module first failed for its missing source file. Mutation checks changed
prompt context, rules heading, trust load count, and skill filename respectively;
each caused a real behavioral comparison failure, then original bytes were
restored. Compaction start text, auto-commit prefix, and checkpoint restore
notification mutations likewise fail their comparisons. Native example paths
above are relative to `crates/pi-coding-agent/`. Only targeted `cargo test -p pi-coding-agent --test example_<name>` and
formatting are run by this implementer; no clippy/check/workspace acceptance.
