# sessions-command.ts traceability

Frozen: `packages/coding-agent/src/cli/sessions-command.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. Whole 186-line module audited against `src/cli/sessions_command.rs`.

| Symbol / branches | Proof |
| --- | --- |
| `handleSessionsCommand`: non-sessions ignored; help/unknown destinations and exit codes | Actual-source `sessions_console_branches_match_frozen`, full output comparison |
| `truncate-tool-output`: exact argc, agent dir, migration report; singular/plural, skipped total, errors/exit1 | Existing native migration and raw-UTF16 error fixtures; invalid argc in actual-source matrix |
| `archive`: parseDays default5/finite/positive/exact flag+argc; refresh before cutoff/archive; singular/plural report | Existing concrete cutoff/native console fixtures and actual-source console matrix (default/fractional/invalid days) |
| Default archive effect compresses each selected file with `archivePersistedSession` | `default_archive_compresses_the_file_and_relocates_metadata`: real JSONL, zstd round-trip, old-path removal, atomic DB metadata relocation/archive state |
| `compressArchivedSessions`: exact no args/--dry-run, ordered candidates/skips, no mutation on dry-run, per-file catch/continue, complete stdout/stderr and exit1 on failures | Six actual-source candidate/report scenarios (empty/single/mixed × dry/live), including both resident ids/directories, precompressed suffix, lookalike directory, success and failure |
| `shouldCompressArchivedSession`, `isResidentSession`: .jsonl only, exact resident id or path segment (slash/backslash) | Same candidate differential |
| `writableMetadata`: loaded fields, ISO created/modified times, explicit invalid-date rejection | Existing `sessions_command_date_tests::writable_metadata_dates_match_frozen_iso_and_invalid_date_errors`; native refresh maps fields into the shared DB writer |
| `formatTruncateToolOutputReport`, `printSessionsHelp` | Native report fixtures and exact help in actual-source matrix; help now includes compress-archived |

RED `/tmp/settings-models-sessions-red.log`: frozen compression command/help absent natively. RED `/tmp/settings-models-sessions-archive-red.log`: default archive returned original path and left file/metadata uncompressed. The shared archive helper now performs the same effect as the frozen default branch. Dependency injection uses existing `SessionMetadata`, not a new carrier; control DB/list/compress effect seams match frozen options.

OS/runtime fault and numeric-extreme shapes remain excluded by the parity bar. Default stdout/stderr exotic-fd failures are in that excluded boundary; no API widening for them. No new dependency, provider call or source delta.

Focused post-commit GREEN and changed-line warning/readability evidence: `/tmp/settings-models-proof-ledger.md`.
