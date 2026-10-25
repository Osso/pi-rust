# Faux — common-pin re-port

Frozen `packages/ai/src/providers/faux.ts`, `4f9e6f8a4` (whole module, lines 1–542). Rust `providers/faux.rs`.

| Exports / branches | Frozen lines | Proof |
|---|---|---|
| `fauxText`, `fauxThinking`, `fauxToolCall`, `fauxAssistantMessage`, normalized content/options | 45–94 | `faux_provider_supports_thinking_text_and_tool_call_blocks`, factory/identity tests |
| context/usage/cache common prefix and cacheRetention | 132–250 | serialized-context/cache tests |
| UTF-16 token chunks, clone retains timestamp 0 | 253–274 | actual frozen `faux_stream_matches_frozen_utf16_chunks_and_zero_timestamp` (unmasked push-boundary case GREEN; arguments remain empty through every delta) |
| stream callbacks, empty queue/factory failures, text/thinking/tools, abort and terminal reasons | 300–401 | fixed chunks/multiple tools/error+aborted/mid-stream abort tests |
| `createFauxCore`: defaults, nonempty model list, empty requested id, queue/state | 403–511 | `faux_empty_model_list_and_empty_requested_id_use_default_model`, queue/state/model-aware tests |
| `fauxProvider`: Models integration and auth | 525–542 | `faux_provider_registers_and_estimates_usage`, rewrite/auth integration tests |

## Deletion and repairs

Frozen whole-tree search for `DeferredHandle`, `fetchDeferred`, `cancelDeferred`, `FauxDeferred` returned no matches. Removed Faux deferred options/state/submit/poll/cancel machinery and its tests; no renamed frozen equivalent exists. RED capability regression now proves absence. Removed old-only Pending test (typed source excludes it). Initial partial preserves message stop reason, matching shallow spread at frozen line 310, rather than forcing Pending. Preserved timestamp 0 (frozen nullish coalescing), repaired empty model/default-id branches, and chunked JSON argument strings by exact UTF-16 units.

## Historical repair proof

Revision `8fe7b79d` + Faux slice. `/tmp/pi-rust-repin-delta-ai/ledger.md`; `faux-repin-red.log`: 3 RED; `faux-utf16-red.log`: new actual-import full-event RED. Current focused target 18 GREEN / 2 RED before last stale initial-reason assertion correction; final count recorded after commit.

**Approved immutable snapshot contract (2026-10-22), binding rule in `AGENTS.md`.** Historical consumer-time comparison substituted six alias-derived fields and passed20 tests (`faux-approved-snapshots-green.log`). Those substitutions are now deleted: the oracle copies actual events at push before queue aliasing, comparing every field. The real RED at42e8875a was event10 native arguments filled before the last delta; frozen fills them only after the delta loop. Removed premature last-delta assignment; unmasked target20 GREEN (`faux-last-delta-green.log`), including exact complete frozen events. No waiver reinstated. [Normalizer matrix](../ai-scripted-provider-oracles.md#provider-oracle-normalizer-audit-2026-10-22).

Whole-Tool JSON also now retains AgentTool label/executionMode. The exact frozen harness fixture reproduces48-vs53 RED and passes53 after the carrier; four variants compare Faux usage, estimator and Tool JSON bytes, including lone-surrogate label data. [Carrier/consumer audit](ai-types.md#enumerable-agenttool-metadata), `tool_metadata` GREEN at eede786f.

**Approved async-factory interleaving deviation2026-10-23**, [AGENTS.md](../../../AGENTS.md):39: native does not reproduce JS microtask ordering between a user-supplied async factory's abort continuation and the agent abort. Usage totals match; input/cacheRead split may differ. Only `thinking_timeout_recovery_matches_frozen_rpc` applies that scoped usage comparison; all output/cacheWrite/cost/totalTokens fields and other RPC values/types/presence remain exact. It also reuses `oracle/assistant-snapshots.ts` at push under the existing immutable-partial approval (AGENTS.md:19). q15-thinking-timeout-rpc-approved-deviation-green-4.log GREEN. No continuation driver or async-factory fallback was added.

## Whole-module closure (2026-10-25)

Audited every export and frozen branch, including model defaults, state/queue ordering, callback-before-step, empty queue, clone/usage/cache construction, each streaming block/abort boundary and provider auth wrapper. Current22focused tests GREEN (`/tmp/ai3-faux-green.log`). Two direct-source fractional-size runs (1.25 and1.3) compare complete immutable push snapshots, UTF-16 chunks, usage and result with empty system prompt/session ID. RED exposed early usize narrowing and incorrect empty-string truthiness. Native JsString convenience content and model display-name construction were compile-RED, then repaired with the existing carriers. Raw string factory rejection is actual-source RED→GREEN with exact unmatched-surrogate error text and zero usage. Callback formatting now uses the existing Error.message/String authority at all three catch sites; exotic toString failure routes to the shared uncaught authority, not a widened API.

GAP: native randomness/ID values, cold/dropped-future scheduling, nonfinite/numeric-extreme options and JS microtask/timer interleaving follow the approved runtime gaps; chunk-size arithmetic, UTF-16 slice positions and usage are not waived. Immutable snapshots and owned values retain existing approvals. Mechanical cross-lane fallout: two FauxModelDefinition display-name literals in interactive_mode.rs tests now construct JsString. No feature remains open.
