# Q18 provider ingress — bounded checkpoint

Frozen source: `4f9e6f8a4`, `packages/ai/src/api/{openai-completions,openai-responses-shared,openai-codex-responses,azure-openai-responses,mistral-conversations}.ts`. Contract: [lossless content](../../specs/lossless-content.md).

| Boundary | Implementation | Proof / remaining |
|---|---|---|
| OpenAI SDK SSE → completions | `SseJsonStream` parses JsValue; text/thinking and function/custom argument fragments remain JsString | Real loopback chunked HTTP, one-byte frames, exact delta order and final units |
| SDK SSE → Responses shared | Native events, slots, done records, refusal/summary joins and citation buffers | Real one-byte loopback text/thinking/argument events; direct Azure + Codex SSE; actual frozen provider stream comparison |
| Codex SSE/WebSocket → shared Responses | Native parse/queue/map/error payload and response normalization | Direct Codex SSE and frozen provider comparison pass; direct real-loopback WebSocket fixture added |
| Mistral SSE → content/arguments | Native parse and argument buffers; frozen ingress sanitizeSurrogates calls retained | Real loopback sanitized text/thinking and raw nested arguments |
| Scalar protocol metadata | Exact protocol paths checked before scalar extraction; unpaired units produce stream error, not replacement/drop | Pending user classification; not full metadata parity |

Raw human SDK/provider error text is a **separate unapproved content/error gap**, not opaque metadata: existing String error records/formatters and serde ProviderError payloads remain scalar. Existing error/status/cancellation semantics are retained; no full raw failure-path parity claim.

No outgoing builder, shared type, core or coding-agent changes. `SdkJsonResponse.data` remains scalar: its only consumer is non-stream OpenRouter image generation, outside this text-stream slice.

## Development ledger

Artifacts: `/tmp/pi-rust-repin-lossless-ingress/`.

- `red.log`: fixture type mistake, not behavioral RED.
- `red-behavior.log`: frozen-baseline completions delta emits U+FFFD instead of D800.
- `responses-red.log`: frozen-baseline Responses delta emits U+FFFD instead of D800.
- `compile-1.log`, `compile-2.log`, `green-attempt-1.log`: intermediate compiler failures; not proof.
- `green-attempt-2.log`: two tests PASS, covering completions, Mistral and Responses; no warnings. Worktree Cargo runner enforces sandbox, fake loopback HTTP only; jobs=4 and worktree target. Later test extensions require a new targeted run.

- `mistral-red-behavior.log`: parent-revision Mistral ingress emits t/U+FFFD rather than the frozen sanitizer's t; corrected native parse now preserves the source sanitation boundary.
- `differential-attempt-1.log`: oracle initially passed fetch as an ignored option; fixed to replace oracle-local global fetch before provider client construction. No live network: sandbox blocked external access.
- `mistral-oracle-diagnostic.log`: original minimal chunk omitted SDK-required model/index/finish_reason/name; corrected test fixture against frozen installed Mistral SDK schemas.
- `ingress-green-2.log`: revision `64dafd44557ba35695246d1085d2c9a3096cd9f1`, five tests PASS, no warnings. All five SSE providers match frozen TypeScript observable event-type order and final content; metadata-like argument keys retain raw units; checked metadata rejection and gated cancellation pass.
- `mutant-replace.log`: SDK scalar-parser replacement compiles; Responses text assertion fails U+FFFD versus D800.
- `mutant-reject.log`: SDK content-as-metadata rejection compiles; expected ToolUse fails against Error. Both mutants restored byte-identically; SDK SHA256 `2b2fa0a11d15d7dfffa8cb170632d25435dec0b4b04025a23d4afcd7a41a7ca0`.

Final direct WebSocket proof and exact revision ledger follow in the lane report. No workspace readiness claim.
