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

These are targeted development diagnostics, not six GREEN suites or whole-module parity. They supersede pre-repin recorded-fetch claims for these paths. Other oracles already intercept globals (Codex/Google) or have different SDK stand-ins (Bedrock); this slice does not revalidate them.

## Approved native interface deviation

User approved `ProviderRequestOptions.fetch` as a Rust-only test/embedder seam on 2026-10-22; binding rule in AGENTS.md. Unset must behave exactly like frozen default fetch. This closes the pending interface decision, not unrelated module gaps. The native seam is never treated as a frozen option or a source-oracle fallback. [Request/stream contract](traceability/ai-types.md#request-contracts-cratespi-aisrcrequestrs-cratespi-aisrchttprs).

## Follow-up identity proof

SDK identity now matches frozen6.26.0. The three OpenAI targets advance past request headers to source-absent rawStopReason (Completions) and native minimum output16 versus source5/4 (Responses/Azure). OpenRouter abort text now matches frozen `Request was aborted.`; its13 behavioral tests and entire current differential are GREEN. Exact revision/commands: ledger, `*-sdk626*.log`. Historical RED table above records discovery, not current acceptance state.
