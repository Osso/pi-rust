# OpenAI Codex Responses

Port the complete frozen `packages/ai/src/api/openai-codex-responses.ts` at `4f9e6f8a4`. [Transport/branch mapping and existing proof](../porting/traceability/openai-codex-responses.md).

## What it must do

- [x] Stream SSE and WebSocket text/reasoning/tool output, usage, errors and stop reasons; preserve immutable per-event partial snapshots under the approved owned-event contract.
- [x] Retain session/account-scoped WebSocket continuation, retry/discard paths, cleanup, request IDs, and source no-SSE-fallback behavior. Existing native37-test baseline is passing; whole-module revalidation remains active.
- [ ] Build the exact frozen request: source-present options only, fixed `tool_choice: "auto"`, null strict tools, and Team included among Responses tool-call providers.
- [ ] Match full diagnostics with each timestamp within that invocation's before/after clock bounds; compare every other field exactly except approved omission of V8 stack text.
- [ ] Complete the whole-module SSE/WebSocket/helper/error branch audit and actual-source proof matrix before promoting the partial row.

Cached continuation eligibility compares serialized request/body-prefix JSON, including property order, exactly as frozen JSON.stringify. Reordered but structurally equal callback payloads must send full context rather than a delta.

JWT payloads follow atob's Latin-1 string then JSON.parse. Truthy account values survive extraction; header coercion/validation occurs after onPayload, preserving source failures and callback ordering without UTF16 projection.

Header construction follows frozen Headers semantics: convert the whole initial record before append validation, append case-insensitive aliases, then apply additional set/delete operations before required Codex headers. Failures occur before HTTP retries, including fields later overwritten; values use HTTP whitespace normalization, ByteString bounds and NUL/CR/LF checks. Preserve separate Set-Cookie entries through SSE and the frozen WebSocket record projection.

Codex ignores cacheRetention. A supplied truthy session ID retains header/cache affinity; empty IDs do not identify a WebSocket cache entry or add SSE session headers. Session headers/request IDs use the raw supplied ID; only body prompt_cache_key is clamped to64code points. Keep empty prompt_cache_key when the supplied ID is empty, matching frozen.

Approved delta (2026-10-24): WebSocket cache identity is `(sessionId, String(accountId))`. A different account adds a separate entry without replacing the first account's socket/continuation. Session cleanup closes every account entry for that session; age/idle eviction remains per entry and debug stats remain per session. The Codex oracle exclusively selects the hash-locked [delta](../porting/delta-lock.json), based directly on the common pin; frozen bytes remain untouched.

Port frozen terminal-status discrepancy literally: a terminal completed/done/incomplete event with failed/cancelled status produces `done` with reason/error and stopReason/error but no invented errorMessage. Caller abort still produces the frozen error event, never done/aborted.

## How it works

- [Current implementation and proof mapping](../porting/traceability/openai-codex-responses.md).

## Implementation inventory

- `crates/pi-ai/src/api/openai_codex_responses.rs`: request, event, retry, SSE/WebSocket and cache contracts.
- `crates/pi-ai/src/api/openai_responses_shared.rs`: shared conversion/event consumer; separately tracked, no unrelated changes authorized.
- `oracle/openai-codex-responses-oracle.ts`: actual frozen module with injected fake HTTP and loopback WebSocket fixtures.

## Tests asserting this spec

- `crates/pi-ai/tests/openai_codex_responses.rs`.
- `crates/pi-ai/tests/differential_openai_codex_responses.rs`.

## Known gaps (current cycle)

- [ ] Finish source/native audit, exact request delta RED/GREEN, timestamp-bounded diagnostic comparison, and final integration gates.

## Out of scope

External endpoints/live providers, unrelated provider APIs or shared-type redesign. Retain only documented approved UTF16 opaque-scalar, owned-event and V8-stack exclusions; new semantic deviations require parent approval.
