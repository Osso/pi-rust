# Mistral conversations traceability

Frozen: `packages/ai/src/api/mistral-conversations.ts` at [source lock](../source-lock.json), 665 lines. Native: `crates/pi-ai/src/api/mistral_conversations.rs`.

## Whole-module audit (2026-10-25)

| Source symbols / branches | Proof |
|---|---|
| `MistralOptions`, `stream`, `createOutput`, per-request client, missing/empty key | original 43 actual-source requests/errors, valid stream snapshots, native behavior tests and shared direct acquisition matrix; raw output uses model.api like source |
| `createMistralToolCallIdNormalizer`, `deriveMistralToolCallId` | nine-character ASCII rule, deterministic hash, per-request collision/replay mapping; original scenario50 and native replay test |
| `buildChatPayload`, `toChatMessages`, `toFunctionTools`, `stripSymbolKeys` | exact callback JSON and SDK wire bytes; user text/images/image-only omission, assistant text/thinking/tools, tool error/image/empty branches; symbol keys excluded by native JSON domain, not invented |
| SDK defaults vs callback boundary | callback sees source role/content/toolCalls and name/description/parameters/strict order, without SDK prefix/index defaults. Only the SDK wire converter adds defaults and reorders function/message fields; new source regression was RED before moving them |
| `buildToolResultText`, explicit surrogate sanitation | source JS trim removes BOM around ordinary tool output; native helper retains JsString through trimming. Existing explicit source sanitizeSurrogates call sites remain the only repair points |
| `buildRequestOptions`, `shouldUsePromptCaching` | no retries; model→caller case-sensitive header merge then native Headers normalization; nullable affinity handling, cache retention/session ID, source SDK UA/Cookie/Accept; source does not call onResponse |
| `streamSimple`, `usesReasoningEffort`, `usesPromptModeReasoning`, `mapReasoningEffort`, `mapToolChoice` | original reasoning request matrix, direct synchronous key rejection, shared base-options proof; exact three effort model IDs, clamp/off mapping, nullish custom mapping (including empty native string) |
| `consumeChatStream`: first ID, usage replacement, cached aliases/cost, finish/choice guard, text/thinking switches, tool identity/partial JSON, final close | five original valid SDK streams plus six new combined valid thinking/text/interleaved-two-tool/usage/unknown-field cases. Each compares full push-time snapshots and final message against the actual frozen SDK |
| `getMistralCachedPromptTokens`, `mapChatStopReason` | all six cache aliases in valid source streams; zero/default/clamp and stop/length/model_length/tool_calls/error/unknown branches have native and original source request/error coverage |
| SSE UTF-8 decoder, boundaries, DONE/EOF/error | original bytewise/boundary/malformed cases; new valid leading-BOM stream in whole chunks and single-byte chunks. Only the initial transport BOM is consumed; the BOM inside content is retained. Before the fix native silently lost the first valid event |
| `formatMistralError`, `truncateErrorText`, `safeJsonStringify`, catch cleanup | HTTP status/body/empty-body/truncation and raw callback/native Error/DOMException matrices; valid tool-then-malformed proof removes partialArgs on catch. Source unknown/error final result remains generic |
| lazy factory | shared [lazy proof](api-lazy.md) and native wrapper dispatch |

## Explicit gaps

| Status | Boundary |
|---|---|
| open SDK gap | Full nested SDK media/tool/response-format transformations, defaults/key order for arbitrary replacement callback payloads, and nested Zod validation/error trees are not fully ported or proven. The Pi-generated paths and valid streamed shapes above are proven, not full SDK acceptance; coverage stays partial. |
| gap | Wrong-typed/exotic SDK JSON, out-of-domain SDK enum strings, reference identity, numeric extremes and OS/runtime fault shapes use the project bar. Header-validation fixtures do not stand in for valid stream coverage. |
| gap | Native HttpResponse always has a body; a fetch implementation returning a missing body has no typed native representation. No new carrier is added for that runtime shape. |

`request.fetch` remains the approved Rust-only injection seam; the fake fetch runs the actual TS SDK through the global fetch hook. No live provider is used. Native error classes/reason behavior are covered by [shared error-class proof](native-error-classes.md), [lossless errors](lossless-provider-errors.md) and [ingress](lossless-provider-ingress.md).

Current proof and exact artifacts: [AI API ledger](../ai-api-repin-proof.md). The old model-less 43 scenarios are retained unchanged: they prove request/error paths, not successful streaming. The additional valid fixtures close that distinction rather than repair those original inputs.
