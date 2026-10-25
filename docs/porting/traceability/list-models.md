# Coding-agent list models traceability

Frozen `packages/coding-agent/src/cli/list-models.ts`, common pin4f9e6f8a4. Native `cli/list_models.rs`. [Spec](../../specs/list-models.md).

| Frozen branch | Native | Proof |
|---|---|---|
| formatTokenCount M/K/integer/fractional/toString | format_token_count/shared JS-number helpers | Actual-frozen table fixtures including toFixed ties, threshold values and padding |
| getError truthiness; console.error before getAvailable | immediate ListModelsIo stderr write | Nonempty/empty warning; concrete TypeError after warning retains complete error and partial stderr |
| empty catalog/no-match | immediate stdout line | Exact guidance/no-match output and stream-write sequence |
| fuzzyFilter over `${provider} ${id}` | one concatenated native key | Single token opengpt spans openai/gpt-4.1; RED→GREEN |
| provider/id locale sort, six headers and flags, UTF-16 padEnd | shared collation/padding, one write per row | Provider/id ordering, Unicode identifiers and complete line snapshots |
| human search/warning/error text | JsString throughout | Lone-surrogate search, warning and availability error with constructor/own code property |
| production caller | StdioListModelsIo + CallbackError::from | Main caller compiles with native error ownership preserved |

`ListModelsRuntime` models the source registry's structural getError/getAvailable dependency. `ListModelsIo` follows the existing `LoginIo` console-injection pattern; it is the production write boundary, not a second capture-output API. Removed ListModelsOutput so a getAvailable failure cannot discard an already-printed warning. Test capture records every actual stdout/stderr call and their concatenated bytes, even when the source rejects. The only substitution remains the differing install directory in no-model guidance.

Proof: q15-list-models-red.log reproduces the cross-field single-token failure. q15-list-models-io-red.log records the missing IO boundary contract before implementation. q15-list-models-green-1.log GREEN2 tests/13 actual-frozen cases. Empty-warning truthiness, single-token search and native UTF-16 are repaired; no source errors or output fields are stripped. No AbortSignal parameter exists in this frozen module; the old traceability claim was stale.

Shared number formatting, collation and Chalk implementations retain their own proof scopes. Targeted clippy/readability and final lane acceptance are not claimed by this checkpoint.

## Whole-module revalidation (2026-10-25)

All111 frozen lines compared: warning before availability failure; empty guidance/search; concatenated fuzzy key; provider/id locale order; UTF-16 widths; token thresholds/formatting; exact writes and flag cells. Added ordinary-redirection color proof: stdout is a regular file while stderr is a real pty. Frozen default Chalk uses stdout color capability, so its warning stays uncolored; native used stderr capability and incorrectly emitted yellow escapes. RED `/tmp/settings-models-list-color-red.log`; native now uses the same stdout color authority, retaining stderr as the warning destination. The pty proof applies only the documented ONLCR console-newline transform to the expected bytes; no ANSI stripping. It reuses the standalone resolver console harness to avoid another binary. No new dependency/API.
