# coding-agent session-cwd traceability

Frozen source: `packages/coding-agent/src/core/session-cwd.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` ([lock](../source-lock.json)). [Contract](../../specs/session-cwd.md).

Rust: `crates/pi-coding-agent/src/core/session_cwd.rs`. Native filesystem/error boundaries use the shared Node adapters; results remain fallible through main/runtime callers. `tests/session_cwd.rs` has 16 tests; `oracle/session-cwd-oracle.mts` invokes the actual unmodified module.

| Frozen symbol/branch | Native mapping | Proof |
|---|---|---|
| `SessionCwdIssue`; private `SessionCwdSource` getter shape | owned issue fields; native trait implemented by SessionManager | real managers plus getter-input fixtures; exact UTF-16 issue comparison |
| `findNearestExistingCwd`: directory, regular file, ENOENT/ENOTDIR ascent | public Result helper; stat follows symlinks and requires a directory | `regular_file_fallback_uses_its_directory_like_frozen` RED/GREEN; `nearest_directory_matches_frozen_stat_branches_and_raw_candidate`: directory/file/missing/non-directory ancestor/empty/root/symlinks/raw surrogate candidate |
| stat throws except ENOENT/ENOTDIR | shared Node argument/stat errors retained by CallbackError | NUL TypeError and actual blocked-directory EACCES RED/GREEN; compare name/message/code/errno/syscall/path/class |
| fixed-point no-directory error | explicit frozen error text | source correspondence only; normal POSIX root/current-directory inodes prevent real-filesystem reproduction |
| `getMissingSessionCwdIssue`: absent/empty file, empty/existing cwd | Result<Option<issue>>; frozen early-return order | empty-file RED/GREEN; `missing_cwd_guards_skip_invalid_fallback_like_frozen` covers absent file, empty cwd, existing directory and existing regular file with an invalid fallback |
| missing stored cwd; nearest fallback | exact stored strings and found candidate | persisted/deleted-directory tests; actual-source issue comparisons |
| `reopenSessionWithCwd`: absent/empty file | explicit error before I/O | empty-file RED/GREEN; metadata file must remain absent |
| reopen with selected cwd, sessionDir and metadata cwdOverride | delegates exact arguments; persists real metadata | `reopen_cwd_and_metadata_match_frozen_for_empty_and_raw_arguments`: default/explicit/empty directories, empty/raw selected cwd, exact returned cwd and real source/native SQLite cwd rows; `asserts_and_reopens_with_selected_cwd` |
| `formatMissingSessionCwdError` / `formatMissingSessionCwdPrompt` | exact JsString construction; empty file omitted only from diagnostic text | literals, raw UTF-16 and empty-file diagnostic RED/GREEN |
| `MissingSessionCwdError`: name/message/issue | original native error retained; rejection record includes exact issue | name/issue RED/GREEN; `raw_missing_cwd_error_matches_frozen_rejection`; original downcast retained for native instanceof-equivalent dispatch |
| `assertSessionCwdExists`: no issue / missing issue / stat throw | CallbackError Result without message flattening | guarded assertion cases, full missing-error record differential, actual filesystem error tests |
| interactive resume catches MissingSessionCwdError itself | caught original issue; reconstruction helper/reopen deleted | actual frozen handleResumeSession/prompt method vs real native runtime; rewrite JSONL after error generation and before UI delivery; confirmation and cancellation frames stay tied to original error |

## Proof ledger

Logs under `/tmp/pi-rust-repin-lossless-interactive/`:

- `session-cwd-{file,nul,stat,name}-red.log`: independent behavior REDs against the frozen module, before production changes.
- `session-cwd-empty-file-{red,green}.log`: three additional truthiness REDs, then GREEN.
- `session-cwd-support-final-development.log`: all 16 module tests GREEN; subsequent legacy SessionManager assertion mismatch is separately reconciled against frozen source, not hidden.
- `resume-cwd-original-error-{red,green}.log`: real runtime/UI error-delivery race RED/GREEN. Confirmation arguments and rendered cancellation status are separate observation contracts.
- [SessionManager audit](session-manager.md#common-pin-truthiness-audit-2026-10-23) records delegated truthiness corrections and their proof.

## Discrepancies and exclusions

No new semantic deviation. The stat guard above remains a proof limitation, not a fabricated dynamic proof or an approved semantic gap. Invalid JavaScript runtime input types and Windows-only paths retain the existing project classifications. Before-run/after-turn runtime recovery is not accepted by this support-module checkpoint.
