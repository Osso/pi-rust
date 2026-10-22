# Faux — common-pin re-port

Frozen `packages/ai/src/providers/faux.ts`, `4f9e6f8a4` (whole module, lines 1–542). Rust `providers/faux.rs`.

| Exports / branches | Frozen lines | Proof |
|---|---|---|
| `fauxText`, `fauxThinking`, `fauxToolCall`, `fauxAssistantMessage`, normalized content/options | 45–94 | `faux_provider_supports_thinking_text_and_tool_call_blocks`, factory/identity tests |
| context/usage/cache common prefix and cacheRetention | 132–250 | serialized-context/cache tests |
| UTF-16 token chunks, clone retains timestamp 0 | 253–274 | actual frozen `faux_stream_matches_frozen_utf16_chunks_and_zero_timestamp` (full event case remains RED only on aliasing fields) |
| stream callbacks, empty queue/factory failures, text/thinking/tools, abort and terminal reasons | 300–401 | fixed chunks/multiple tools/error+aborted/mid-stream abort tests |
| `createFauxCore`: defaults, nonempty model list, empty requested id, queue/state | 403–511 | `faux_empty_model_list_and_empty_requested_id_use_default_model`, queue/state/model-aware tests |
| `fauxProvider`: Models integration and auth | 525–542 | `faux_provider_registers_and_estimates_usage`, rewrite/auth integration tests |

## Deletion and repairs

Frozen whole-tree search for `DeferredHandle`, `fetchDeferred`, `cancelDeferred`, `FauxDeferred` returned no matches. Removed Faux deferred options/state/submit/poll/cancel machinery and its tests; no renamed frozen equivalent exists. RED capability regression now proves absence. Removed old-only Pending test (typed source excludes it). Initial partial preserves message stop reason, matching shallow spread at frozen line 310, rather than forcing Pending. Preserved timestamp 0 (frozen nullish coalescing), repaired empty model/default-id branches, and chunked JSON argument strings by exact UTF-16 units.

## Current proof and blocker

Revision `8fe7b79d` + Faux slice. `/tmp/pi-rust-repin-delta-ai/ledger.md`; `faux-repin-red.log`: 3 RED; `faux-utf16-red.log`: new actual-import full-event RED. Current focused target 18 GREEN / 2 RED before last stale initial-reason assertion correction; final count recorded after commit.

**Approved immutable snapshot deviation (2026-10-22), binding rule in `AGENTS.md`.** Frozen lines 330–336 and 386–389 emit shallow partial copies sharing content blocks. Actual-import consumer oracle observes final text in text_start/early text_delta, and final arguments in early toolcall_delta. User approved native immutable per-event snapshots instead. Differential substitutes exactly six concrete partial fields: event 1 text `""`, event 2 text `"abcd"`, events 6–9 arguments `{}`; every other field stays exact. No event/field is dropped or ignored. GREEN: 20 tests, including native lone-surrogate/astral full-event differential. Log `faux-approved-snapshots-green.log`.

Additional unproven boundaries: random chunk distribution/IDs, scalar `FauxContent::Text` convenience input, fractional/nonfinite token-size runtime values. These are not silently approved exclusions.
