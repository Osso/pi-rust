# coding-agent external-session-importer traceability

Pinned source: `packages/coding-agent/src/core/external-session-importer.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust module: `crates/pi-coding-agent/src/core/external_session_importer.rs`.
Focused tests: `crates/pi-coding-agent/tests/external_session_importer.rs` (6), using concrete Codex and Claude fixture files for each supported format.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `EXTERNAL_SESSION_PROVIDERS` | `codex`, `claude` | `ExternalSessionProvider` |
| `isExternalSessionAlias` | `<provider>/<id>` with a known provider and non-empty id without `/` | `is_external_session_alias`; `recognizes_external_aliases` |
| `importExternalSessionAlias` | find source, destination `<sessionDir>/<provider>-<id>.jsonl`, reuse existing file, default session dir | `import_external_session_alias`; `imports_a_codex_session_alias`, `imports_a_claude_session_alias`, `reuses_an_existing_imported_session` |
| `parseExternalSessionAlias` | split on first `/` | private `parse_external_session_alias`; alias test |
| `findSourceSession` | recursive `.jsonl` search under `~/.codex/sessions` / `~/.claude/projects`, filename match, id prefix match | private `find_source_session`; codex/claude import tests |
| `fileNameMatchesAlias` | exact, `-id` suffix, or contains | private `file_name_matches_alias` |
| `readCodexSession` | `session_meta.payload.id/cwd`, filename fallback, `response_item` messages | private `read_codex_session`; codex test |
| `readClaudeSession` | first `sessionId`/`cwd`, file-name id fallback, `message` rows | private `read_claude_session`; claude/fallback tests |
| `codexLineToMessage` / `claudeLineToMessage` | only `response_item`/message payloads; provider-specific assistant metadata | private helpers; import tests |
| `messageLikeToAgentMessages` | user/assistant only, text blocks, `Date.parse` timestamp or now, zero usage, `model:"imported-session"`, `stopReason:"stop"` | private helper; import and fallback tests |
| `normalizeTextContent` | string, `text`/`input_text`/`output_text` blocks only | private helper; fallback test |
| `writeImportedSession` | v3 header with `parentSession = source.path`, chained message entries, persistence serialization | private helper; claude JSONL assertions |
| `findIdInCodexFileName` | regex `([uuid]|[A-Za-z0-9._-]+)\.jsonl$` | private helper (whole stem when allowed, trailing UUID otherwise) |
| `sanitizeFileName` | non `[A-Za-z0-9._-]` → `-` | private helper; destination path assertions |

## Gaps

- The source function is `async`; the Rust function is synchronous because it performs only synchronous filesystem work.
- `Date.now()` fallbacks use the system clock; tests pass explicit timestamps.

## Discrepancies

None.

## Q18 native NodeFS error identity (2026-10-22)

Importer read/scandir/mkdir/open error helpers receive original native path/dir;
only the concrete FS operation uses its encoded argument. A missing raw-name
import file fixture checks error text retains original units. Written but
unexecuted while coding compilation remains blocked.
