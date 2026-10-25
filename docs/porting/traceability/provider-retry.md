# Native SDK retry helper

`packages/ai/src/utils/provider-retry.ts` belongs to the retired pin, not common pin4f9e6f8a4. `crates/pi-ai/src/utils/provider_retry.rs` remains a Rust implementation helper for the current OpenAI/Anthropic SDK stand-ins; the retired helper is not the parity authority.

Current authorities: frozen installed `openai/client.mjs:433–470` and `@anthropic-ai/sdk/client.mjs:377–415`, reached through public SDK requests in `oracle/stainless-retry-oracle.ts`.

| Branch | Current proof |
|---|---|
| x-should-retry override, absent status,408/409/429/>=500; fresh request/attempt count | existing native retry tests and whole provider request matrices |
| retry-after-ms parsed first; truthy retry-after replaces an absent or zero millisecond hint | new actual OpenAI and Anthropic public API calls both schedule200ms for headers0 and0.2 seconds; native public retry operation must really wait at least180ms and perform exactly two calls |
| missing hint uses bounded exponential delay/jitter; date header uses current clock | existing native delay/date tests; timing/random scheduling edge cases retain the project runtime boundary |
| SDK wrappers disable the native helper's old server-delay cap | OpenAI Responses/Completions/Azure/images regressions retained; Anthropic now also ignores source-unused maxRetryDelayMs, proven with2ms hint and caller old cap1ms |

**Correction:** neither current frozen SDK has the older60s hint-selection restriction. No such restriction was added. Their default-delay branch is `timeoutMillis === undefined`; zero can be replaced by the seconds header but does not independently select default jitter.

The native helper still exposes its internal optional cap for existing Rust callers; it is not forwarded as an OpenAI/Anthropic SDK option. Platform timing, entropy failure, pathological headers/numeric extremes and Node timer interleavings retain the project gap boundary. Existing reason-bearing signals are unchanged; no CancellationToken exception applies.

Proof: `differential_stainless_retry::zero_millisecond_hint_uses_seconds_header_like_both_actual_sdks`, affected full source-provider renewal, [lane ledger](../ai-api-repin-proof.md). The source fixture schedules request timeouts normally and fast-forwards only observed retry timers. A3s request watchdog and immediate fatal exit bound the fixture; no network is used. An initial wrong SDK export selection caused a fixture startup hang, not behavioral RED; all cargo/Node process groups were cancelled and verified gone before the corrected fixture ran.
