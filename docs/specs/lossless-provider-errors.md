# Lossless human provider errors

Human provider error text retains exact JavaScript UTF16 units through the frozen OpenAI SDK, completions, Responses/Azure, Codex SSE/WebSocket and Mistral error chains. Source and bounded proof: [traceability](../porting/traceability/lossless-provider-errors.md).

## What it must do

- [x] Preserve unmatched human error units in `AssistantMessage.error_message`, emitted Error events and JSON persistence.
- [x] Preserve SDK inner error payloads and human `metadata.raw`; structured JSON formatting follows source stringification.
- [x] Keep parser error message data authoritative, including excerpts split across surrogate pairs.
- [x] Preserve callback, retry-event/retry-limit and diagnostic human text; UTF16 retry/overflow classification retains frozen character semantics.
- [x] Keep Mistral HTTP truncation exact by code-unit count; retain frozen content sanitizer behavior.

## How it works

- [API, source boundaries and proof matrix](../porting/traceability/lossless-provider-errors.md)

## Implementation inventory

- `crates/pi-ai/src/types.rs`, `request.rs`: assistant/image errors, callbacks and retry events.
- `crates/pi-ai/src/api/{openai_sdk,openai_completions,openai_responses_shared,openai_responses,azure_openai_responses}.rs`: SDK and Responses error mapping.
- `crates/pi-ai/src/api/{openai_codex_responses,mistral_conversations}.rs`: native stream, HTTP and parser errors.
- `crates/pi-ai/src/utils/{error_body,provider_retry,retry,overflow,diagnostics}.rs`: formatting, retries and persisted diagnostics.
- Other pi-ai callback/auth/image consumers adapt the shared human-error API without broadening their independent payload contracts.

## Tests asserting this spec

- `crates/pi-ai/tests/lossless_provider_errors.rs`
- `crates/pi-ai/tests/lossless_ingress.rs`
- `crates/pi-ai/tests/{error_body,provider_retry,retry,overflow,diagnostics}.rs`
- `oracle/lossless-provider-errors-oracle.ts`

## Known gaps (current cycle)

- [ ] Reconcile pre-existing Mistral `onResponse` invocation mismatch and remaining base native-content fixture drift at the full-port integration gate; see [classification](../porting/traceability/lossless-provider-errors.md#pre-existing-port-discrepancy).

## Out of scope

Opaque metadata policy/approval, nonstream image payload ingress, coding-agent consumers, whole-provider parity and broad acceptance gates remain outside this bounded human-error slice.
