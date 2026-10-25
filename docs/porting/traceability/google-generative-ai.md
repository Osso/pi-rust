# Google Generative API traceability

Frozen: `packages/ai/src/api/google-generative-ai.ts` at [source lock](../source-lock.json),525 lines. Native: `crates/pi-ai/src/api/google_generative_ai.rs` and the shared `google_client.rs` SDK path.

## Whole-module audit (2026-10-25)

| Source symbols / branches | Native proof |
|---|---|
| `GoogleOptions`, `createClient`: truthy API key, model base URL/version, model→caller headers, GoogleGenAI construction | original45 source request/event scenarios; missing/empty raw key now rejects before HTTP; scoped typed options and exact native headers |
| `buildParams`: converted contents, optional temperature/clamped ceiling/system/tools, explicit tool choice only with tools, thinking config, pre-aborted guard, model/contents/config | exact source wire body strings; [shared conversion proof](google-shared.md); no strict/constrained schema rewrite or provider retry wrapper |
| `buildThinkingConfig`, `usesServerManagedThinking` | latest alias omits thinkingConfig for both enabled/disabled paths; uppercase alias and Flash-Lite control cases; original enabled/level/budget requests |
| `isGemini3ProModel`, `isGemini3FlashModel`, `isGemma4Model`, disabled config | unanchored case-insensitive source matching, aliases and Gemma4; prefixed model-name source regression; exact gemini-3.7-flash off→LOW, other Flash off→MINIMAL, Pro LOW, Gemini2 budget0 |
| `getThinkingLevel`, `getGoogleBudget`, `streamSimple` | all four effort levels, Pro/Gemma collapse rules,2.5 Pro/Flash-Lite/Flash budgets/custom/dynamic default, model clamp and off→high; direct Simple auth rejects acquisition via Result; base options preserve cwd |
| `stream`: fixed output API, payload replacement, SDK acquisition, start, first responseId, text/thinking switches/signature retention, current-block close | full original/new source event/result fixtures; callback raw/native rejection matrix; request headers/body remain native to SDK boundary |
| function calls: close current text/thinking, accept provided unique ID, generate for absent/empty/duplicate ID, empty name/args defaults, signature, start/delta/end | existing tool/signature cases plus empty-ID and absent-name regressions; per-module counter is proven in an isolated process, not hidden by timestamp normalization |
| per-chunk finish reason/toolUse override, usage replacement/cost, final close/abort/unknown error/done/catch | full source stream matrices including partial/body/error/abort/usage cases; native push-time snapshots retain approved ownership semantics |
| lazy factory | [shared lazy proof](api-lazy.md), native lazy-wrapper behavior |

**DISCREPANCY preserved:** absent functionCall.name produces a generated ID prefixed `undefined_` while the emitted tool name is empty; the old native empty ID prefix was not frozen behavior.

## Explicit gaps

- The approved [typed-native payload signal row](google-client.md#typed-native-payload-signal-boundary) applies; no new carrier is introduced.
- SDK global base-URL process-env overrides are implemented but not independently isolated in the current differential. SDK host-version headers, OS/runtime fault shapes, pathological wrong-typed JSON/numeric extremes and unmatched units beyond existing opaque/schema carriers retain the project gap boundary. Arbitrary args that are not objects are excluded by the typed core.

The wire fixture compares body text without JSON/error normalization; only volatile clocks/generated-ID times and host Node version substrings are normalized. A separate process proof compares actual counter domains exactly. [Ledger](../ai-api-repin-proof.md); [SDK scope](google-client.md).
