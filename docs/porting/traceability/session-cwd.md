# coding-agent session-cwd traceability

Pinned source: `packages/coding-agent/src/core/session-cwd.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust module: `crates/pi-coding-agent/src/core/session_cwd.rs`.
Focused tests: `crates/pi-coding-agent/tests/session_cwd.rs` (5).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `SessionCwdIssue` | sessionFile/sessionCwd/fallbackCwd | `SessionCwdIssue` |
| `getMissingSessionCwdIssue` | no file or existing cwd → undefined; nearest existing fallback | `get_missing_session_cwd_issue`; `detects_missing_session_cwd_from_persisted_sessions`, `uses_nearest_existing_parent_when_cwd_was_deleted` |
| `reopenSessionWithCwd` | reopen with cwd override and control-db cwd | `reopen_session_with_cwd`; `asserts_and_reopens_with_selected_cwd` |
| `formatMissingSessionCwdError` | exact multi-line error | `format_missing_session_cwd_error`; `formats_errors_and_prompts` |
| `formatMissingSessionCwdPrompt` | exact prompt | `format_missing_session_cwd_prompt`; same test |
| `MissingSessionCwdError` | message + `name` + issue | `MissingSessionCwdError` (Display + `NAME`); same test |
| `assertSessionCwdExists` | throws on issue | `assert_session_cwd_exists`; `asserts_and_reopens_with_selected_cwd` |

## Gaps

- `reopenSessionWithCwd` accepts `control_db_path` but does not call `setMetadataControlDbPath`; the
  method and wiring are owned by the in-flight `ca-hosts-fixups` branch (not merged at this slice's
  base). The in-memory cwd override is applied through `SessionManager::open`.

## Discrepancies

None.
