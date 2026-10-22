# Q18 provider ingress — bounded checkpoint

Frozen source: `4f9e6f8a4`, `packages/ai/src/api/{openai-completions,openai-responses-shared,openai-codex-responses,azure-openai-responses,mistral-conversations}.ts`. Contract: [lossless content](../../specs/lossless-content.md).

| Boundary | Implementation | Proof / remaining |
|---|---|---|
| OpenAI SDK SSE → completions | `SseJsonStream` parses JsValue; text/thinking and function/custom argument fragments remain JsString | Real loopback chunked HTTP, one-byte frames, exact delta order and final units |
| SDK SSE → Responses shared | Native events, slots, done records, refusal/summary joins and citation buffers | Real loopback text/thinking/argument events; Azure direct smoke pending |
| Codex SSE/WebSocket → shared Responses | Native parse/queue/map/error payload and response normalization | Compiled by focused target; direct transport regression pending |
| Mistral SSE → content/arguments | Native parse and argument buffers; frozen ingress sanitizeSurrogates calls retained | Real loopback sanitized text/thinking and raw nested arguments |
| Scalar protocol metadata | Exact protocol paths checked before scalar extraction; unpaired units produce stream error, not replacement/drop | Pending user classification; not full metadata parity |

No outgoing builder, shared type, core or coding-agent changes. `SdkJsonResponse.data` remains scalar: its only consumer is non-stream OpenRouter image generation, outside this text-stream slice.

## Development ledger

Artifacts: `/tmp/pi-rust-repin-lossless-ingress/`.

- `red.log`: fixture type mistake, not behavioral RED.
- `red-behavior.log`: frozen-baseline completions delta emits U+FFFD instead of D800.
- `responses-red.log`: frozen-baseline Responses delta emits U+FFFD instead of D800.
- `compile-1.log`, `compile-2.log`, `green-attempt-1.log`: intermediate compiler failures; not proof.
- `green-attempt-2.log`: two tests PASS, covering completions, Mistral and Responses; no warnings. Worktree Cargo runner enforces sandbox, fake loopback HTTP only; jobs=4 and worktree target. Later test extensions require a new targeted run.

Metadata rejection and content replacement/rejection mutants, Azure/Codex direct cases, and final exact revision ledger remain pending. No workspace readiness claim.
