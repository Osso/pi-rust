# Model mailbox tools — frozen 4f9e6f8a4

Source `extensions/agents-core/src/mailbox-tools.ts`; native registration/adapters in `agents_core/mailbox_runtime.rs`.

| Export / branch | Proof |
| --- | --- |
| ContactParentMessageParams and SendAgentMessageParams | Native transport input keeps message, fileRefs, threadId, toAgentId, optional toSessionId/toHost with lossless JsString/JsValue carriers |
| fileReferenceSchema, mailboxParameters, contactParentSchema/sendAgentMessageSchema | Exact frozen 12-tool registration metadata differential, including required nonblank body, descriptions, additionalProperties=false and host/session fields |
| createSendAgentMessageTool/createContactParentTool | Required body maps literally to transport message; no assistant-text/message fallback. `question_relay::mailbox_body_and_remote_host_are_literal_and_unknown_local_sessions_are_rejected` checks verbatim whitespace body, remote recipientHost, local rejection and no extra queue writes |
| outgoingBodyGuideline and cross-host prompt guideline | Exact same registration metadata differential |
| registerModelMailboxTools: send then contact, registration failure propagation | Same native registration order and Result propagation; all metadata/order matches source |

Current proof: both `differential_agents_core_deterministic` tests pass; all nine `question_relay` integration tests pass (`/tmp/agents-core-relay-focused.log`). Routing helper has its own [whole-module source/native same-DB proof](session-message-route.md). Transport/persistence algorithm ownership remains with runtime/store modules; this adapter does not invent alternate routes.

Gap: malformed/exotic runtime argument shapes follow the project parity boundary; model-facing schema validation remains exact.
