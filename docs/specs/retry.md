# Frozen AI retry classifiers

Authority: read-only `packages/ai/src/utils/retry.ts` at [source lock](../porting/source-lock.json), commit `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

- `is_codex_cybersecurity_restriction` requires error stop reason, exact `openai-codex-responses` API, and a nonempty error beginning with the case-sensitive canonical cybersecurity sentence, optionally prefixed by exact `Codex error: `. Arbitrary suffixes are permitted.
- `is_retryable_assistant_error` requires error stop reason and nonempty error. Cybersecurity restrictions and all 10 nonretryable source alternatives take precedence over all 41 retryable alternatives. Match source case-insensitive regex semantics, including UTF-16 dot, JavaScript whitespace, exact anchors, WebSocket-101 and Daybreak Blue optional request-ID syntax.
- ResourceExhausted, getaddrinfo, ENOTFOUND and EAI_AGAIN alone are not retryable. No retry policy, callbacks, sleeps or retry loop are exported by this source module.
- Inputs use existing typed `AssistantMessage`; invalid JavaScript runtime input types remain outside the approved typed boundary. No new type exclusions or semantic deviations.

## Staged-removal blocker

Historical Rust `RetryPolicy`, `RetryCallbacks`, `retry_assistant_call` and private loop helpers remain unchanged solely because coupled harness/compaction consumers require them to compile. Their removal is authorized by the overall re-pin, but consumer conversion is outside this slice. This is not compatibility, fallback, waiver or whole-module completion. Existing native loop tests are historical regression checks, not evidence for this pin. Dependencies and exact proof are tracked in [retry traceability](../porting/traceability/retry.md).
