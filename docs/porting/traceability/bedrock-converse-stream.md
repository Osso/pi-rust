# Bedrock ConverseStream traceability

Frozen: `packages/ai/src/api/bedrock-converse-stream.ts` at [source lock](../source-lock.json), 1084 lines. Native: `bedrock_converse_stream.rs` plus existing credential, SigV4 and eventstream helpers in `crates/pi-ai/src/api/`.

## Whole-module audit (2026-10-25)

| Source exports / branches | Current proof |
|---|---|
| `BedrockThinkingDisplay`, `BedrockOptions`, `stream`, `streamSimple` | 17 native behaviors, original whole-provider matrix and shared base/budget proofs; raw model.api output is fixed to Bedrock like source; Simple intentionally forwards only base/reasoning/budget fields |
| configured region/credentials, standard endpoint region, endpoint selection, profile, bearer/skip auth, custom headers | exact actual-Smithy requests/signatures. Four new RED/GREEN cases cover empty region/bearer/profile, explicit profile with configured env credentials, and scoped profile versus actual ambient profile. Explicit credentials win over profile; ambient AWS_PROFILE reads process env, not scoped overrides |
| SDK endpoint/credential feature headers | exact SDK identity/attempt/auth headers; explicit standard endpoints also carry N. Old notes saying only nonstandard hostnames carry N were incomplete; the scoped-profile source regression proves the standard explicit case |
| `buildAdditionalModelRequestFields`, GovCloud, Claude/adaptive/xhigh/caching/signature/model-name helpers | original adaptive/budget/display/interleaved/GovCloud/alias/inference-profile cases; current native Model.name carrier remains native; default budgets, custom overrides and off/base clamp preserved |
| cache retention, system prompt, required/nonblank text, message conversion, consecutive tool results, tool config and image blocks | exact wire strings and the recorded command objects; last-user/system cache points and TTL; source empty placeholders, thinking-signature/plain-text fallback and non-Anthropic omission; tool schemas keep empty keys and have no old strict/constrained rewrite |
| `normalizeToolCallId` | new astral-ID replay RED/GREEN compares full command payload and wire body; ASCII replacement and64-unit cap operate on UTF16 units, not Rust scalar characters |
| `createImageBlock` error propagation, JS text trim | unsupported SVG now fails before onPayload/HTTP, not silent omission; private conversion/build functions propagate Result. A BOM-only user string becomes the source empty placeholder, not retained text |
| eventstream decode, message start, content start/delta/stop, metadata, stop mapping/catch cleanup | full exact push-time snapshots, fractional-index legacy fixture, tool JSON/signatures, cost/usage and end/error cases; raw event JSON uses existing JsValue, not scalar serde Value. New real-Smithy raw frames prove unmatched UTF16 text, thinking and argument values through full events and final output |
| `formatBedrockError`, reserved headers, retries and native reason boundaries | parsed SDK service errors versus raw gateway bodies, data-retention hint, five named exception prefixes; retained 30 actual source/native real-socket abort cases across pre-abort/pending headers/body; no invented diagnostics/rawStopReason |

**DISCREPANCY preserved:** parsed real Node HTTP service exceptions lose the raw response body through normalizeProviderError's consumed IncomingMessage and stringify as `[object Object]`; raw unparseable gateway text is retained. The source loopback fixture reproduces this without error masking. [Earlier provider-oracle audit](../ai-scripted-provider-oracles.md).

## Recorded SDK/runtime gaps

| Status | Boundary |
|---|---|
| gap | Full AWS default credential providers, SSO/assume-role failure paths, scoped proxy/request-handler behavior and SDK region/config chain combinations are not fully proven or ported; static env/profile signing and the listed config paths are proven. Browser/non-Node SDK construction has no native browser runtime. |
| approved gap | Source onPayload image bytes are runtime Uint8Array objects; the JSON-only native callback carries base64. Parent classified this under the typed-native callback approval on 2026-10-25. Wire bytes still match exactly; no new carrier is added. |
| gap | General SDK serialization of arbitrary replacement payloads is not complete: modelId replacement routing, unknown/nested-field filtering and raw callback status/body normalization require more proof/porting. Existing supported payload replacement/capture does not prove these branches. |
| gap | Native resolves profile credentials before building payload/onPayload; source SDK authentication is lazy. The SDK provider-chain object and missing-profile/callback I/O ordering remain a recorded runtime boundary, not a pending proof expansion; static-env wire paths remain exact. |
| gap | Missing/exotic typed HTTP bodies, pathological wrong-typed JSON, numeric extremes, OS/runtime fault shapes, IDNA edges and Node scheduling oddities use the project bar. No new production carrier is added for those shapes; human protocol values retain the existing native carrier. |

Latest proof: 27 focused tests PASS: 17 native behaviors and10 differential/normalization tests. These include32 source request/content/config scenarios plus30 source/native abort-boundary cases. The independent bearer-signature assertion now models source truthiness/env resolution; it was stale after the successful production fix. The generic captured-payload assertion compares source null when conversion fails before callback, rather than assuming every callback runs. [Exact ledger](../ai-api-repin-proof.md).
