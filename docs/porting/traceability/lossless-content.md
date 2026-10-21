# Q18 lossless shared message boundary

Contract: [lossless content](../../specs/lossless-content.md). Frozen source: `4f9e6f8a4`, read-only.

| Capability | Implemented | Missing / blocked | Proof |
|---|---|---|---|
| `TextContent.text` / `ToolResultMessage.details` | `JsString` / `Option<JsValue>`, explicit-null adapter | downstream consumers pending | `pi-ai/lossless_content`: exact surrogate units, nested keys, omission/null, Rust→TS→Rust-visible JSON |
| Shared serde and object fields | raw-fragment writer/parser; object-only shared adapters | non-JSON serde formats unsupported | `pi-js-compat/js_json_serde`: five focused sandbox cases |
| Content extraction and text streaming final/delta fields | lossless `JsString` | user/thinking/custom strings not migrated yet | pi-ai crate compiles; general provider target regressions pending |
| Provider JSON bodies and `OnPayload` | lossless callback type, scalar builder move into AST; pi-messages direct lossless Context serialization | callback fixtures downstream pending | captured actual HTTP request text/details/callback unmatched-unit escapes equal Node `JSON.stringify` |
| Explicit provider sanitation | frozen per-call-site drop behavior | broad provider regressions pending | pinned `sanitize-unicode.ts` compared against Rust on the same message units |
| Node UTF-8 stream/path/process boundary | shared `encode_node_utf8` | consumer migration pending | distinct high/low-unit replacement test; original authority unchanged |
| Agent loop / JSONL / questions / mailbox / session name | pending next checkpoint | no end-to-end claim | not yet proven |

## Development proof ledger

Artifacts under `/tmp/pi-rust-repin-lossless-content/`:

- `serde-red.log`: shared serde API missing (compiler RED); `serde-green.log`: original three cases pass.
- `content-red.log`: original ToolResultMessage rejected a lone surrogate at deserialize (behavioral RED).
- `helpers-red.log`: new explicit boundary/object helper APIs missing (compiler RED).
- `js_json_serde-green.log`: five focused helper/serde cases pass.
- `content-provider-diff.log`: oracle fixture failed because TSX selected CJS and rejected top-level await; fixed fixture, not counted as behavioral RED.
- `content-provider-diff-green.log`: two sandbox differentials pass, including actual pi-messages HTTP body capture and callback replacement.

Checkpoint proof is bounded. It does not establish workspace compilation, unchanged provider test fixtures, all-source parity, or JSONL integration.

User/thinking checkpoint: `UserContent::Text`, `ThinkingContent.thinking`, thinking delta/end events use JsString. OpenAI Completions conversion/cache/request builders now use JsValue, preserving the source-unsanitized reasoning-signature field. `user-thinking-red.log` reproduces rejection of a user lone surrogate; `user-thinking-green-2.log`: four pi-ai sandbox tests PASS, including captured OpenAI HTTP reasoning escapes. Initial thinking-only HTTP fixture was dropped by the existing no-visible-content rule; adding visible text reproduces the actual source boundary. Custom text, argument API and downstream consumers remain pending.

## Argument/outbound checkpoint

`ToolCall.arguments` is a JsObject field using shared object serde. Complete/partial/repair JSON parsers return JsValue; grammar input extraction/buffering retains JsString. Native Anthropic/Google/Bedrock request trees use JsValue; OpenAI/Mistral argument JSON text uses the lossless writer rather than serializing bare map keys. Google raw SSE decoding selects typed records from lossless JSON, including JsValue arguments and JsString text.

Proof: `arguments-red.log` reproduces the old ToolCall argument-key rejection. `arguments-checkpoint-green.log`: seven focused tests PASS, zero warnings. One test captures real HTTP bodies from five direct providers (Anthropic, Google, OpenAI completions/responses, Mistral), confirms exact argument units and byte equality to Node JSON.stringify. Another differential loads frozen `utils/json-parse.ts` for complete and partial unmatched-unit cases. Bedrock request migration has compilation evidence only at this checkpoint.

A generic dispatcher fixture hung after a Google provider task panicked on a bare JsObject macro leaf. The Cargo group was terminated and all children verified exited; partial proof is `arguments-provider-green-2.log` (five prior cases passed, new case unproven). Bounded direct-provider reproduction (`arguments-provider-direct.log`) identified the key-serialization panic; construction now wraps the object in JsValue, with no guard/fallback. `arguments-provider-direct-green.log` passes.

### Checked-boundary status

No whole-value argument/event adapters remain. Validator integration at `1120b034` uses the lane's JsValue engine directly; its entrypoint now constructs JsValue::Object(call.arguments.clone()). Anthropic events are JsValue throughout.

`anthropic_messages::metadata_text` remains a checked UTF-8 restriction for scalar provider metadata (ids/names/type discriminators, stop reasons/explanations, signatures). Parent approved this explicit gap pending user classification on 2026-10-21. Unmatched units produce an error, never replacement. Text, thinking and argument content never call this helper.

### Native fragment / Anthropic ingress checkpoint

`ToolCall.partial_json` and ToolCallDelta text use JsString; provider argument buffers and complete/repair/partial parsers operate on native UTF-16. `anthropic-ingress-red.log` reproduces the removed event adapter rejecting a valid text delta. `anthropic-ingress-green-attempt.log`: eight sandbox cases PASS, zero warnings, including actual ordered SSE message/text/thinking/tool-fragment/delta/stop handling. The tool fragment carries a native unmatched unit after outer JSON parsing; its final argument retains that exact unit. Frozen partial-parser differential additionally covers raw unmatched units in complete and unterminated strings.

Workspace compilation, consumer fixtures and end-to-end agent/JSONL/question/mailbox/name proof remain pending.

## Provider-native JSON checkpoint

`ProviderNativeContext.value` uses JsValue. Responses shared conversion/tools and OpenAI/Azure/Codex request trees now retain native items without projection into serde_json::Value; Codex continuation response-item cache uses the same AST. The core custom-message extractor can decode the typed native context from its lossless authority rather than dropping valid unmatched units.

`native-context-red.log` reproduces the old native value deserialize rejection. `native-context-green.log`: ten pi-ai sandbox tests PASS, zero warnings, including a captured actual Responses HTTP body with unmatched-unit native text and key, no scalar fallback, and exact Node JSON.stringify bytes. Other existing provider fixtures remain pending migration/regression execution.
