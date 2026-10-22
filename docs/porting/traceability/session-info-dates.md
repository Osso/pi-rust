# SessionInfo Date representation and consumers

Common pin `4f9e6f8a4`; [contract](../../specs/session-info-dates.md). Implementation reference for V8 sorting is separately [locked](../v8-array-sort-source-lock.json), not a TypeScript pin change.

`SessionInfo.created` and `modified` now hold f64 Date time values, including NaN. `PartialEq` no longer incorrectly derives Eq. Raw message timestamps/lastActivity stay f64; only actual new Date(number) construction invokes shared `date::time_clip`. Shared ISO conversion returns `InvalidTimeValue` (RangeError/Invalid time value), and SessionManagerError retains that name/message rather than replacing the failure with an epoch or dropping the session.

| Frozen consumer / exact boundary | Native mapping | Pinned proof |
|---|---|---|
| session-manager.ts:1046–1070 finishSessionInfo; :1073–1099 file loading | raw header Date conversion; positive activity→TimeClip; valid string-header/fallback Date; pre-epoch filesystem mtime | `file_session_info_preserves_invalid_dates_and_timeclip`: invalid header, positive out-of-range activity, fractional activity/header, negative activity and pre-epoch fallback; exact time values/ISO outcomes |
| session-manager.ts:1172–1184 metadata Date construction | date_parse preserves NaN, no unwrap_or(0) | `metadata_loading_and_cache_iso_failure_match_frozen`: real control DB metadata→SessionInfo |
| session-manager.ts:1110–1125,1199–1204,1668–1679 metadata ISO/write | shared fallible toISOString in cache/snapshot; conversion before SQL write | same metadata/cache test; `append_metadata_keeps_numbers_until_the_frozen_iso_failure_boundary`: actual public append/DB read over NaN/±Infinity/large/fractional values, exact outcome/name/message, entry count and stored-row state |
| session-manager.ts:1259–1260,2380,2419 date sorting | shared pinned JS sort at inner/outer sort boundaries | `filesystem_session_order_preserves_frozen_invalid_date_comparisons`: real7/70-file lists, exact full order/Date outcomes |
| cli/session-picker.ts:74–92 loadSessions | unchanged archive/name priority, f64 date comparator through JS sort | `picker_date_sort_matches_frozen_with_invalid_dates`:7,17,32,70,129 rows |
| cli/sessions-command.ts:162–175 writableMetadata | fallible ISO construction, no invalid-date saturation | `cli::sessions_command::date_tests::writable_metadata_dates_match_frozen_iso_and_invalid_date_errors`: five cases incl NaN/Infinity/range limits and fractional TimeClip; actual frozen pure function extracted with pinned TypeScript compiler |
| session-selector.ts:40–53 formatSessionDate; :220–280 tree aggregation/sort | f64 floor/format, NaN-propagating subtree max, JS sort | `selector_invalid_date_render_and_subtree_order_match_frozen`: exact frames/selected path for recent/threaded, invalid child and extreme valid dates |
| session-selector-search.ts:209–216 relevance/threaded modified-date tie | numeric literal/score/date comparator through JS sort | `search_date_ties_match_frozen_with_invalid_dates`: same five lengths × all three sort modes |
| interactive-mode.ts:6639–6646 sortNamedSessionsFirst | same named-first/f64 date comparator via JS sort | comparator structure shared with picker; no separate whole-mode differential claimed |

The oracle executes real frozen exports/classes. Only private pure CLI functions are extracted by exact AST declaration and transpiled, with original imported resident predicate for picker; no hand-written function replica. Current-clock append timestamps are normalized solely as stored-created/stored-modified equality (`<header>`); fixed epoch ISO values and failures remain exact.

Consumer/identity audit found no built-in Date mutator or identity comparison: consumers read getTime/toISOString or render helpers. Frozen `core/idle-session-archive.ts:193–194` is an additional ISO consumer with no Rust module in this checkout; that separate unported source row is not silently implemented or claimed by this scoped Date slice.

## Discrepancies preserved literally

| Classification | Frozen behavior | Handling/proof |
|---|---|---|
| DISCREPANCY — source UI bug | Invalid Date age falls through all comparisons and renders NaNy | preserved in exact selector frame, not replaced with a friendly fallback |
| DISCREPANCY — source inconsistent comparator | Date NaN returns NaN/equal and exposes V8-specific ordering | pinned [TimSort](array-sort.md), not Rust stable sort/total_cmp;840 exact order+callback-trace cases |

## Proof ledger

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:

- `test-session-info-dates-file-red.log`: epoch0 substitution, dropped out-of-range Date, wrong numeric header date. Consumer/API REDs: `test-session-info-date-consumers-red.log`, `test-sessions-command-date-red.log`.
- `test-session-info-dates-green-2.log`:3/5 passed; picker/search reproduced Rust-vs-V8 ordering mismatch at7 rows. This failure motivated the exact shared algorithm, not relaxed expectations.
- `test-session-info-dates-green-3.log`:5/5 after V8 binding. Additional filesystem-order1/1 (`test-session-info-filesystem-sort-green.log`), append/DB-boundary1/1 (`test-session-info-append-boundary-green.log`).
- Final expanded file case1/1 (`test-session-info-dates-file-green-final.log`, includes pre-epoch fallback); CLI metadata1/1 (`test-sessions-command-date-green-1.log`); existing fractional timestamp regression1/1 (`test-session-info-timestamp-regression-green.log`).
- `test-v8-array-sort-green-1.log`:840 comparator/order/trace cases. No broad-suite, clippy or final acceptance claim is made by these targeted development proofs.
