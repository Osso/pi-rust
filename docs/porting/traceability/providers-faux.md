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

## Current proof and blocker

Revision `8fe7b79d` + Faux slice. `/tmp/pi-rust-repin-delta-ai/ledger.md`; `faux-repin-red.log`: 3 RED; `faux-utf16-red.log`: new actual-import full-event RED. Current focused target 18 GREEN / 2 RED before last stale initial-reason assertion correction; final count recorded after commit.

**Approved immutable snapshot contract (2026-10-22), binding rule in `AGENTS.md`.** Historical consumer-time comparison substituted six alias-derived fields and passed20 tests (`faux-approved-snapshots-green.log`). Those substitutions are now deleted: the oracle copies actual events at push before queue aliasing, comparing every field. The real RED at42e8875a was event10 native arguments filled before the last delta; frozen fills them only after the delta loop. Removed premature last-delta assignment; unmasked target20 GREEN (`faux-last-delta-green.log`), including exact complete frozen events. No waiver reinstated. [Normalizer matrix](../ai-scripted-provider-oracles.md#provider-oracle-normalizer-audit-2026-10-22).

Whole-Tool JSON also now retains AgentTool label/executionMode. The exact frozen harness fixture reproduces48-vs53 RED and passes53 after the carrier; four variants compare Faux usage, estimator and Tool JSON bytes, including lone-surrogate label data. [Carrier/consumer audit](ai-types.md#enumerable-agenttool-metadata), `tool_metadata` GREEN at eede786f.

Additional unproven boundaries: random chunk distribution/IDs, scalar `FauxContent::Text` convenience input, fractional/nonfinite token-size runtime values. These are not silently approved exclusions.
