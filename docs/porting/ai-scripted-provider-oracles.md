# Q4 provider oracle fetch boundary

Reference: [common pin](source-lock.json), `4f9e6f8a4`. Revision `a2b0a400` + oracle-boundary correction. Full outputs and exact commands/revisions: `/tmp/pi-rust-repin-delta-ai/ledger.md` and `*-global-fetch.log`.

## Root cause and repair

Six differentials supplied `options.fetch` to frozen API modules. Their SDK constructors do not consume this old option (e.g. frozen `api/anthropic-messages.ts:813–907`). The modules loaded, but the scripted response paths were not exercised: native fetch was blocked by the mandatory sandbox and produced connection errors. This was stale oracle infrastructure, not evidence of response/event parity.

`oracle/scripted-fetch.ts` now installs a global dispatcher **before** source/SDK import. Each request supplies its recorded fetch script and clears it in `finally`. Fetch without an active script fails explicitly; no native fetch fallback exists. Mistral's recorder now reads SDK `Request` method/URL/headers/body rather than treating it as a URL string. All existing equality/payload/event assertions remain; two failure messages additionally print parseable full JSON for diagnosis. No fixture fields were added/deleted to avoid a source validation error.

## Re-execution matrix

| Target | Corrected source execution | Current RED, not waived |
|---|---|---|
| `differential_anthropic_messages` | scripted request + normal SSE, scenario 0 | Native terminal event/result add source-absent `rawStopReason`; every other scenario-0 value equal |
| `differential_openai_completions` | actual SDK request headers, scenario 0 | Native `x-stainless-package-version` 6.40.0; frozen 6.26.0 |
| `differential_openai_responses` | actual SDK request headers, scenario 0 | Same SDK-version drift |
| `differential_azure_openai_responses` | actual SDK request headers, scenario 0 | Same SDK-version drift |
| `differential_mistral_conversations` | actual SDK Request and SSE schema, scenario 0 | Frozen SDK rejects fixture missing `data.model`; native accepts. Also source SDK user-agent/cookie and unused onResponse differences. No assertion narrowed |
| `differential_openrouter_images` | scenarios 0–2 equal; abort scenario 3 reached | Native `Request aborted`; frozen `Request was aborted.` |

Remaining audit: Google, Codex, legacy aliases, and provider-dispatch oracles now install the same guarded global fetch before import and clear scripts after each request. Provider dispatch records SDK `Request` bodies/headers rather than stringifying Request objects. Existing exact assertions remain. Legacy aliases pass; Google, Codex, provider-dispatch, and the unchanged Bedrock stand-in differential remain RED (`*-remaining-fetch-audit.log`, ledger). These failures are not waived.

These are targeted development diagnostics, not six GREEN suites or whole-module parity. They supersede pre-repin recorded-fetch claims for these paths. Other oracles already intercept globals (Codex/Google) or have different SDK stand-ins (Bedrock); this slice does not revalidate them.

## Approved native interface deviation

User approved `ProviderRequestOptions.fetch` as a Rust-only test/embedder seam on 2026-10-22; binding rule in AGENTS.md. Unset must behave exactly like frozen default fetch. This closes the pending interface decision, not unrelated module gaps. The native seam is never treated as a frozen option or a source-oracle fallback. [Request/stream contract](traceability/ai-types.md#request-contracts-cratespi-aisrcrequestrs-cratespi-aisrchttprs).

## Follow-up identity proof

SDK identity now matches frozen6.26.0. The three OpenAI targets advance past request headers to source-absent rawStopReason (Completions) and native minimum output16 versus source5/4 (Responses/Azure). OpenRouter abort text now matches frozen `Request was aborted.`; its13 behavioral tests and entire current differential are GREEN. Exact revision/commands: ledger, `*-sdk626*.log`. Historical RED table above records discovery, not current acceptance state.

## Provider oracle normalizer audit (2026-10-22)

Reference: [common pin](source-lock.json),4f9e6f8a4. Scope: AI provider request/event/result oracles and matching Rust wrappers. Historical GREEN counts only prove their then-normalized boundaries. No scratch-field exclusion is approved. Immutable push-time snapshots are approved; identity/mutation-through-reference is excluded unless a frozen built-in consumer relies on it.

### Findings

| Oracle / native wrapper | Projection | Status / required proof |
|---|---|---|
| OpenAI Completions/Responses/Azure shared API oracle | Recursively removed partialJson/customInput/streamIndex/partialArgs | Removed. Unmasked Responses6/Completions16 RED. Restore native scratch lifecycles; full event objects compared |
| Responses shared module | Same recursive scratch deletion | Removed. Shared stream2 RED; direct callback already captures at push |
| Codex | Source scratch deletion; both sides removed partial.responseId | Removed. Tool-call scratch RED. WebSocket responseId RED revealed read-time aliasing, not proof of missing native state |
| Mistral | Both sides recursively remove partialArgs and stack | Removed on both sides, including deterministic-number canonicalization. Existing SDK validation RED remains visible |
| Bedrock | Rust wrapper strips block-shaped index/partialJson | Removed. SDK UA is now exact; only explicit signing clock/date, invocation UUID and derived signature are masked, with separate exact signature verification |
| Anthropic | Timestamp-only message normalization; clones events when consumed | No scratch waiver. Install push-boundary cloning so queue aliasing cannot alter snapshots before capture |
| Google | Generic timestamp/generated-id normalization; JWT iat/exp and environment SDK headers | Generic traversal removed; generated IDs are normalized only at assistant.content[*] toolCall.id. Message clocks use known event/result paths; SDK UA/version now exact |
| OpenRouter images | Request header/error normalization | No assistant scratch filtering found |
| Provider registry / legacy aliases | Descriptors, auth and request/result capture, no partial-event capture | No scratch filtering found |
| Supplementary lossless ingress/errors | Bounded human-text/argument output projections, not whole-event proof | Guard fetch before import and apply push-boundary snapshots; main provider matrices own full-event parity |
| Faux | Six documented alias-derived expected-field substitutions | Removed all six expected-field substitutions; actual source push-boundary cloning now supplies immutable snapshots |

### Push boundary, not field waivers

`oracle/assistant-snapshots.ts` wraps the frozen AssistantMessageEventStream prototype before provider import. Only the event handed to the queue is structured-cloned; source content/state and frozen files are unchanged. This applies the approved owned-event contract at push instead of approximating it by deleting responseId or scratch fields after async iteration. The affected oracle consumers only read event/result data; no mutation-through-returned-reference dependency is used. Mistral already captures on the prototype push boundary; Google has an instance push recorder. Neither needs a second recorder.

### Allowed paths and pending native drifts

- Clock masking is limited to event.partial/message/error.timestamp and terminal result.timestamp (Mistral/Bedrock call the terminal value message). Request bodies and nested argument JSON are untouched. A concrete request/argument fixture reproduced recursive timestamp corruption RED.
- Generated Google IDs are masked only at assistant.content[*] blocks tagged toolCall, field id. Arbitrary arguments.id remains exact. OAuth JWT masking stays at the form assertion's root claims.iat/exp and signature; header/remaining claims and ordering are compared.
- OpenAI host masks are limited to captured request headers x-stainless-os/arch/runtime/runtime-version, and only when neither model nor caller configured that header. Stable SDK user-agent/version values remain exact.
- Bedrock retains all header fields. Only captured x-amz-date, amz-sdk-invocation-id, SigV4 credential date and derived signature are masked; algorithm, principal, region/service and signed-header list stay exact. Exact signature recomputation separately checks the frozen request. Bearer authorization and SDK identity remain exact.
- All scratch/index/stack and malformed-error-text projections are removed. Native partialJson/partialArgs/streamIndex/index lifecycles and newly unmasked SDK/error differences remain work to port, never waiver candidates.

Development evidence: `/tmp/pi-rust-repin-delta-ai/provider-scratch-unmasked-red.log`; exact commands/revisions in the lane proof ledger. These findings remain open until their native behaviors and unmasked tests pass; this audit is not whole-provider parity evidence.

