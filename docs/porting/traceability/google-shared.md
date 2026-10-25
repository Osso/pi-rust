# Google shared API traceability

Frozen: `packages/ai/src/api/google-shared.ts` at [source lock](../source-lock.json),382 lines. Native: `crates/pi-ai/src/api/google_shared.rs`.

## Whole-module audit (2026-10-25)

| Source exports / branches | Proof |
|---|---|
| `GOOGLE_MAX_OUTPUT_TOKENS`, `clampGoogleMaxOutputTokens`, `usesServerManagedThinking` | actual-source helper matrix covers below/at/above65535 and exact case-insensitive latest alias; both owning API request builders exercise omission of explicit thinking |
| `GoogleThinkingLevel`, `isThinkingPart`, `retainThoughtSignature` | enum serialization; eighteen helper cases include thought true/false/absent, signature-only parts and absent/empty/new incoming signatures |
| base64 validation, same-provider/model signature resolution, Gemini unsigned-call sentinel | direct message fixtures and existing native signature/replay tests; only Studio Gemini thinking calls receive the sentinel; Vertex does not |
| `requiresToolCallId`, private Gemini-version/multimodal helpers | case-sensitive Claude/gpt-oss IDs; per-UTF16-unit replacement and64 cap; direct source Claude astral-ID fixture and Gemini2/live3/non-Gemini image routing |
| `convertMessages`: string/block users, assistant text/thinking/function calls, empty-turn suppression | direct actual-source/native serialized values including exact role→parts order; signed empty/blank text/thinking blocks are dropped even with valid signatures, matching frozen body, not the old native retention behavior |
| tool results: text/error/empty/image placeholder, optional ID, supported nested images vs separate image turn, merge only into a user turn already containing functionResponse | direct full-value source fixtures plus native image/merge/error cases |
| `sanitizeForOpenApi`, `convertTools` | no tools→undefined/None; plain native descriptions/parametersJsonSchema; legacy parameters removes eight meta keys recursively from objects but not array elements; exact ordered source/native values |
| `mapToolChoice`, `mapStopReason`, `mapStopReasonString` | all17 declared finish reasons plus explicit unknown rejection, three choices/default, raw-string fallback in eighteen source helper cases |

Human text remains native until frozen's explicit sanitizeSurrogates call sites. Arbitrary argument JSON and provider-native values stay native. The old retry and constrained/strict-schema helper APIs are absent, not alternate paths. The shared Rust-only generate-content builder is covered by the owning API request fixtures.

## Explicit gaps

- Pathological wrong-typed JSON, numeric extremes, OS/runtime fault shapes, unmatched units beyond existing opaque/schema carriers and JS reference aliasing use the project user-observable boundary. The shared [transform timestamp gap](transform-messages.md) is unchanged; no additional carrier/API is added.

Current proof: five direct shared-module tests plus the owning66 original API scenarios, new replay/thinking/config/counter cases and34 native shared/API behaviors. Exact commands/revisions are in the [AI API ledger](../ai-api-repin-proof.md).
