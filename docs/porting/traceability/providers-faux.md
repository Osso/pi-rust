# faux provider traceability

Source: `packages/ai/src/providers/faux.ts` → `crates/pi-ai/src/providers/faux.rs`.
Tests: `crates/pi-ai/tests/faux_provider.rs`; differential `tests/differential_ai_runtime` (`faux_sequence`).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `FauxModelDefinition`, `FauxTokenSize`, `FauxDeferredOptions`, `RegisterFauxProviderOptions` | same | multi-model/deferred/default tests |
| `FauxContentBlock` helpers `fauxText/fauxThinking/fauxToolCall` | `faux_text/faux_thinking/faux_tool_call` returning `AssistantContent` | block helper tests |
| `fauxAssistantMessage` | `faux_assistant_message` + `FauxContent`, `FauxAssistantMessageOptions` | all faux tests |
| `FauxProviderState`, `FauxResponseFactory`, `FauxResponseStep` | same (`Message` boxes the large message) | queue/factory tests |
| `createFauxCore` | `create_faux_core` → `FauxCore` | defaults, stream, deferred tests |
| `stream`, `streamSimple` | `ProviderStreams::stream/stream_simple` | stream tests |
| `fetchDeferred`, `cancelDeferred` | same trait methods | deferred tests |
| `getModel` overload | `get_model(Option<&str>)` | multi-model/default tests |
| `setResponses/appendResponses/getPendingResponseCount` | same | queue tests |
| `fauxProvider` | `faux_provider` → `FauxProviderHandle` | provider tests |
| `registerFauxProvider` | out of scope (`compat.ts` global registry) | gap |

## Semantics
- Responses are consumed in queue order; an exhausted queue yields `"No more faux responses queued"` with estimated usage.
- Messages are cloned and rewritten to the requested api/provider/model before usage estimation.
- Usage estimation serializes the context exactly as TS (`system:`, `role:text`, `tools:` joined by blank lines), estimates `ceil(UTF-16 units / 4)`, and implements session prompt-prefix caching unless `cacheRetention === "none"`.
- Event order for fixed chunk sizes matches TS: start, thinking/text/toolcall start/delta/end, done or error.
- Pending stop reasons produce `"Faux response ended without a stop reason"`. Explicit `error`/`aborted` stop reasons terminate with an error event.
- Abort checks occur before the stream, before each block, and after each paced chunk. Paced chunks use `tokensPerSecond`; otherwise scheduling yields to the executor (TS `queueMicrotask`).
- Deferred submissions store the scripted step, return a deferred handle, and honor `pendingFetches`/`pollAfterMs`. Fetch failures are returned in-band; cancellation is recorded and later fetches report `"Faux deferred response was cancelled: ..."`.
- Factory errors become terminal error messages.

## Representation differences
- `Math.random` chunk sizes and generated ids are replaced by a deterministic xorshift generator and a counter. Tests that assert exact events fix `tokenSize` and tool-call ids; the differential uses fixed chunk sizes.
- `FauxResponseFactory` receives an immutable `FauxProviderStateSnapshot` (`callCount`, `deferredFetchCount`) instead of the live TS state object. Factories cannot mutate provider state.
- Token splitting slices Rust characters, not UTF-16 code units; a chunk boundary can never create an unpaired surrogate. ASCII and all tested inputs are identical.
- Timestamps are real `Date.now`-equivalent values and are excluded from differential comparisons.

## Gaps
- `registerFauxProvider`/`unregister` live in `compat.ts` (global API registry), which is outside this slice.
- `Math.random`-derived ids and chunk boundaries are not bit-identical (non-deterministic in TS).

## Discrepancies
- None observed by differential `faux_sequence` (fixed chunks, session cache, cache-retention-none).
