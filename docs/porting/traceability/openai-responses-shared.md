# Shared Responses algorithms traceability

Frozen: `packages/ai/src/api/openai-responses-shared.ts` at [source lock](../source-lock.json),733 lines. Native: `crates/pi-ai/src/api/openai_responses_shared.rs`.

## Whole-module audit (2026-10-25)

| Source symbols / branches | Native / proof |
|---|---|
| `ConvertResponsesMessagesOptions`, `convertResponsesMessages`: nonempty optional system prompt; developer role unless explicitly false; provider-native input bypass; string/block/image users; assistant thinking JSON, text/signature/fallback IDs, function calls; emitted-call filtering; tool text/image results | existing12 conversion scenarios plus empty system prompt, Unicode legacy signature length, astral ID replacement, extra-pipe split and corrupt reasoning JSON regressions. Native Result carries JsString diagnostics through Responses/Azure and mechanically through Codex callers. |
| `normalizeIdPart`, foreign item IDs, split call/item IDs | per-UTF16-unit ASCII replacement,64-unit cap, trailing underscores; foreign shortHash; first two pipe segments; same-provider/different-model pairing omission; exact source conversion fixture |
| `encodeTextSignatureV1`, `parseTextSignature` | v1/legacy signature handling, optional commentary/final_answer phase; source text/thinking replay fixtures |
| `ConvertResponsesToolsOptions`, `convertResponsesTools` | function-only tool mapping, unmodified parameters/native description, strict absent→false/explicit bool/null; seven exact source scenarios |
| citation-state helpers: raw marker grammar, punctuation/whitespace separation, partial-marker buffering, visible text, finish/closed/full normalization | native UTF16 state; existing hosted-search fixture/unit cases plus split raw citation without any web_search item. Removed source-absent search-call gating. |
| `OpenAIResponsesStreamOptions`, `processResponsesStream`, output slots | native event source and content-index slots; original12 push-time scenarios and new regressions; source has only thinking/text/function slots, no old custom/deferred grammar slots |
| reasoning summary/text deltas, part.done separators, output text/refusal, argument deltas/done suffixes, image partials | direct native payload/scratch transitions; exact source events; whole provider fixtures retain argument scratch at start/delta and strip it at item.done/catch |
| item.done reasoning summary/content/existing fallback and signature; text full normalization/suffix/end/signature; function parse/end/delete | exact original stream matrix and five whole-provider unfinished/error/argument-replacement cases; phase changes signature only, not stopReason |
| terminal finalize: response ID/image result, optional usage replacement, unconditional cost calculation and service-tier callback resolution, stop map/toolUse | original usage/status scenarios; terminal-without-usage reprices existing usage regression; Responses actual-source service-tier request/response matrix |
| incomplete max-output vs explicit reason error; error event template; response.failed error/details/defaults; no-terminal error | original failure cases plus empty failure error defaults regression |

**DISCREPANCY preserved:** an empty tool result without images emits `(see attached image)` (frozen407). It is not corrected to a more helpful placeholder.

Old-only final-answer phase stop reset, hosted-search gating, custom/deferred/namespace/tool-search behavior and final-response encrypted-reasoning backfill are removed. Malformed nonempty thinking replay JSON now fails explicitly with the frozen parse message rather than silently dropping content. Every newly repaired behavior has a genuine frozen-oracle RED in the [proof ledger](../ai-api-repin-proof.md).

## Explicit gaps

- Pathological wrong-typed/adversarial JSON, callback runtime throws beyond registered native callback domains, signed-zero/nonfinite Map indices, numeric extremes, OS/runtime fault shapes and unmatched units beyond existing opaque/schema carriers follow the user-observable bar; no new API is added to model them.
- Events carry approved immutable push-time partial snapshots rather than JS mutation-through-previous-event references. Existing native descriptions, argument JSON and provider-native values stay lossless.

Oracles no longer strip scratch, normalize errors, or rewrite request data. Only volatile message clocks/host versions are normalized in owning provider fixtures. Shared fixture clocks are explicit. Historical broader claims about custom tools, deferred fields and hidden scratch are retired.
