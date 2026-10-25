# session-history.ts

Frozen [4f9e6f8a4](../source-lock.json); native `crates/pi-coding-agent-storage/src/core/session_history.rs`. Whole-module realistic-input correspondence audited2026-10-25; whole manager/control integration acceptance remains separate. [Contract](../../specs/session-history.md).

| Frozen export/private branch | Current proof |
|---|---|
| `readSessionHistoryReverse`: branch newest first, exact parent, no parent/path, missing ancestor, stop at root | `repin_storage_history`4 tests; source-created compacted/archive ancestor and persisted history-tool integration |
| `readPlainEntriesReverse`/`parseEntry`:64KiB backward scanning, blank/header/malformed lines, incomplete final tail and UTF-8 lines spanning chunks | four plain/archive × present/missing ancestor cases after both managers open; existing1.35MiB Unicode differential and13-byte decoded stream proof |
| compressed spool, archive selection and cleanup on normal/error/drop | large Unicode early-drop fixture removes its private spool, preserves archive bytes and creates no plain sibling; corrupted archive after manager creation preserves code/errno |
| plain open rejection, iterator failure exhaustion | exact source/native ENOENT after three retained entries; no error-to-empty-history fallback |

## Current RED→GREEN

- `missing_compacted_transcript_preserves_frozen_open_error` opens the same TS-created compacted transcript in both managers before frozen removes it. Both emit three retained branch entries first. Native formerly flattened the original Node open error into OS Display text. `/tmp/storage3-history-open-{red,green}.log` proves the corrected message; SessionManagerError retains constructor/code/errno/syscall/path.
- `corrupted_archive_history_preserves_frozen_decoder_error_and_cleanup` opens the same frozen-created archive before the oracle replaces its bytes. Text already matched; `/tmp/storage3-history-codec-record-red.log` proves the native error lost code/errno. History now uses the existing spool byte-copy authority (visibility-only change in session-transcript-spool). `/tmp/storage3-history-green-final.log`3 PASS. The earlier codec-text probe passed and is not RED evidence.
- `reverse_history_ignores_malformed_inactive_and_missing_ancestors` passes all4 plain/archive × present/missing ancestor combinations (`/tmp/storage3-history-filtering-valid-green.log`). The initial fixture corrupted the transcript before manager loading; frozen rejects that load. The corrected fixture opens both managers before rewriting physical history, without relaxing any assertion. Initial failure is not production RED evidence.

`/tmp/storage3-compacted-green-final.log`4 PASS renews source-created active ancestry, persisted history-tool integration and large Unicode/early-drop proofs. The in-process spool deadline has its separate [proof](session-transcript-spool.md).

Strict fixture no-emit typecheck passes; formatting/import checks pass with7 inherited any warnings. Manual changed-line Rust/TS readability audit found no new structural issue; metrics tool was unavailable. Strict storage clippy reports74 inherited diagnostics and zero in history/scanner/spool; storage lib cargo check passes. Exact revisions/commands: `/tmp/storage3-proof-ledger.md`. No broad suite or independent acceptance is claimed.

## Earlier parent truthiness correction

`empty_parent_does_not_open_removed_transcript` uses a frozen-created compacted session with `parentId:""`. Both managers open before source removes the transcript; both return the three retained entries without opening physical history. Native initial parent/path and subsequent parents use frozen truthiness. Earlier RED/GREEN: `/tmp/pi-rust-repin-storage/history-empty-parent-{valid-red,green}.log`. The current four-test history module retains that assertion.

Gap: adversarial persisted shapes, exceptional filesystem/tempdir faults and runtime scheduling differences follow the current parity exclusions; no new fault carriers or scheduling emulation were introduced.
