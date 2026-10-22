# coding-agent resume-session traceability

Pinned source: `packages/coding-agent/src/core/tools/resume-session.ts` at `532f7d12`.

Rust: `crates/pi-coding-agent/src/core/tools/resume_session.rs` (`ResumeSessionTool`).

| Source | Rust | Tests |
|---|---|---|
| `resumeSessionSchema` | `parameters()` | differential |
| `readNonEmptyString` / `normalizeResumeSessionParams` | same names | `rejects_blank_target_strings`, missing/multiple target tests, differential |
| `assertResumeSessionPath` | same | missing/non-jsonl tests, differential |
| `findUniqueSessionMatch` | `find_unique_session_match` | ambiguous name test |
| `listLocalAndSessionDirSessions`, `listAllResolvableSessions` (path-keyed merge) | `list_local_and_session_dir_sessions`, `list_all_resolvable_sessions`, `merge_by_path` | id/name resolution tests |
| `findMetadataSessions` (`findActiveSessionMetadataByName/Id`) | same | stale metadata test |
| `sessionsInCurrentDirectory` (`resolvePath` prefix) | `sessions_in_current_directory` | id/name resolution |
| `findResumeSessionById` (exact, then prefix, ambiguity) | same | `resolves_id_targets_across_a_custom_session_directory`, change-cwd exact-prefix test |
| `resolveResumeSessionFile` (path, name scopes, id) | same | id/name/path tests |
| `findSessionFileById` | `find_session_file_by_id` | reused by `change_working_directory` |
| `assertResumeTargetIsDifferent` | same | self-resume test |
| `switchSession` + optional `withSession` starter prompt | `ExtensionContext::switch_session` + `SwitchSessionOptions::with_session` sending through `ReplacedSessionContext` | starter/cancel tests, differential |
| result text/details/`terminate` | `execute` | differential |

Differential: `tests/differential_session_tools.rs::resume_session_tool_differential`
(by id prefix, by name, by path, starter prompt, current path/id/name, ambiguous name,
missing path, both targets, no target, blank name, cancelled switch).

## Q18 native checkpoint

Common pin: `4f9e6f8a4`. Target IDs/paths, path-keyed merge, lookup errors, call/result rendering and current-session comparisons use `JsString`. Path resolution now returns `Result`; process-CWD/path failures propagate rather than using empty-CWD/original-path fallbacks. `find_session_file_by_id` takes native ID/DB path and returns native path. The coupled change-working-directory caller propagates errors.

`lossless_name_tests::path_and_id_targets_keep_original_units` covers trimmed raw target IDs/paths; the existing raw-name lookup assertion remains. Tests are unexecuted pending coding-library compilation; earlier-pin differential evidence above is not new-pin acceptance.

## Source drift queued for classification

Frozen `assertResumeSessionPath` first resolves the transcript read path and accepts `.jsonl.zst`; Rust currently validates only `.jsonl` and does not invoke the archived-transcript resolver. Native migration does not silently correct this behavior.

## Renderers

Renderers ported: `render_call` (target + starter marker), `render_result` (tool output). Covered by `differential_interactive_components.rs::tool_renderer_cases`.
- `SessionManager.list`/`listAll` metadata integration is still partial in the port
  (`control_db_path` is accepted but the scan is used). `resume_session` therefore resolves
  metadata-only sessions through `findActiveSessionMetadata*` and file scans; differential
  fixtures cover file-backed sessions and stale metadata.
