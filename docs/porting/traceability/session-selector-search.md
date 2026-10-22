# session-selector-search.ts

Frozen `4f9e6f8a4`, `packages/coding-agent/src/modes/interactive/components/session-selector-search.ts:1–219`.
Rust: `crates/pi-coding-agent/src/modes/interactive/components/session_selector_search.rs`.

| Frozen symbols / branches | Native mapping | Proof |
|---|---|---|
| whitespace/lowercase and search text, 26–32 | JsString normalization and exactly id/name/firstMessage/cwd | full raw-unit surface differential; existing filtered-ID differential |
| hasSessionName/hasSessionMessages/name filter, 34–46 | same predicates | raw name, whitespace-only name, zero messages, all/named filters |
| parseSearchQuery, 49–124 | UTF-16 token loop, phrase/fuzzy flush, unclosed quote whitespace fallback; optional shared JsRegex i mode | 15 surface queries include empty/JS whitespace/phrase/mixed/unclosed/regex/empty-error/invalid-error/lookahead/backreference/raw pattern |
| isLiteralTokenQueryMatch/matchTokenQuery, 126–159 | phrase normalized index score; fuzzy score; all-phrase/all-fuzzy/mixed literal rules | exact matches/literal/numeric score for every fixture, including pair/unpaired text |
| matchSession regex/empty/token branches, 162–182 | UTF-16 regex match offset and exact error/no-match behavior | same surface differential |
| filterAndSortSessions, 185–219 | name/messages filtering, unchanged empty-query order, recent stable literal partition, relevance/threaded score/date sort | all six sort/filter combinations per query plus existing23-case filtered-ID differential |

All 219 source lines compared. Removed former fancy-regex error-text claim: native JsRegex now produces one raw UTF-16 diagnostic prefix. Reason wording and engine resource limits follow [approved regex contract](regexp-diagnostics.md); Empty regex remains an exact non-RegExp parser error. No later-message accumulator participates in search.

GREEN `/tmp/pi-rust-repin-lossless-interactive/test-session-search-surface-green-2.log`1/1 (15 complete parse/match/sort scenarios), `test-session-search-filter-green.log`1/1 (23 filtered-ID cases). Surface test compares error presence and exact prefix units while excluding only the approved reason text; all other outputs remain exact.

## Active Date follow-up

SessionInfo still carries i64 milliseconds at this checkpoint. Parent authorized the queued f64/TimeClip/Invalid Date migration, including this consumer, after219 integration. Date-comparator Invalid Date behavior is not yet proved; row remains partial until that test and carrier migration land.
