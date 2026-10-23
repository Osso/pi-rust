# Traceability: `packages/agent-core/src/agent.ts`

Reference: immutable `4f9e6f8a4`. Rust: `crates/pi-agent-core/src/agent/agent.rs`.
Evidence and exclusions: [slice-1 proof ledger](agent-core-api-repin.md). Complete with approved owned-value/event-snapshot and typed-input exclusions. Live-state mutation by frozen built-ins is explicitly mapped to native setters below.

| Frozen surface/function | Rust | Proof |
|---|---|---|
| QueueMode, AgentOptions, Agent | re-exported QueueMode, AgentOptions, Agent | construction/state/options tests |
| defaultConvertToLlm, DEFAULT_MODEL, EMPTY_USAGE, createMutableAgentState | default_convert_to_llm, default_model, owned initial state, failure message builder | default/custom state and unsupported-default-provider tests |
| PendingMessageQueue enqueue/hasItems/drain/clear | PendingMessageQueue equivalents | all/one-at-a-time queue tests |
| constructor provider callbacks and mutable public configuration | explicit on_payload/on_response/on_retry; Rust setters for provider functions, hooks, budgets, retry cap, transport/session/tool mode | frozen callback/config test; existing forwarding tests |
| state, steeringMode, followUpMode | snapshot/accessor methods, queue-mode setters | state/queue tests; mutation identity excluded |
| subscribe | subscribe/unsubscribe, Arc callback identity dedup, live monotonically ordered subscription iteration | live Set and duplicate listener regression |
| steer/followUp/clear queues/hasQueuedMessages/signal/abort/waitForIdle | corresponding methods | steering interruption, continuation queue and awaited listener lifecycle tests |
| reset | reset, returns unit even during active run, keeps run reservation | active reset regression and pending-response test |
| prompt/normalizePromptInput | prompt/prompt_with_images/normalize_prompt_input, PromptInput | text/message/batch/image fixtures |
| continue | continue_run, ContinueOptions | steering first, skip initial drain, follow-up first when requested, restored pending tools |
| runPromptMessages/runContinuation/createContextSnapshot/createLoopConfig | corresponding Rust functions | agent and 35-scenario differential |
| runWithLifecycle | begin_run/finish_run_with/finish_run | concurrency rejection, idle settlement, failures |
| handleRunFailure | build_failure_message and failure event sequence | preserve thinking/text/tool call/usage/identity on steer/abort/stream failure; one assistant start; Q18 `lossless_provider_errors::agent_failure_keeps_state_listener_and_persisted_message_native` proves exact UTF16 error text through state, listener and serialized final message |
| processEvents | process_events | state reduction, truthy error text, awaited listeners; log rejection and continue |
| prepareNextTurn wrappers | build_prepare_next_turn_hook | context-hook precedence and dynamic replacement after turn listeners |

The removed process-global get/setDefaultStreamFn API has no frozen counterpart. Default streaming calls pi-ai/compat directly. Rust support types (PromptInput, ContinueOptions, AgentInitialState, listener/hook aliases) express TS overloads/inline structural types; they do not add old runtime behavior.

## Built-in live-state consumer audit

Frozen coding-agent `sdk.ts:438` message assignment maps to native sdk.rs `agent.set_messages`; `agent-session-runtime.ts:518` maps to agent_session_runtime.rs `set_messages`. Frozen agent-session.ts message assignments/pushes (2037,3942,4475,4852,5074,5121,5683,5729,5860,5915,5948,6180) map to native agent_session.rs explicit `set_messages`, including push-equivalent read/append/set at bash recording and queued messages. Frozen tools assignments (2476,2858,2869), systemPrompt assignments (2480,3178,3182,5222), model assignments (4150,5276,5687) and thinkingLevel assignment4239 map to explicit native setters and active-tool override methods. SDK restore, agent-session compaction/branch/generation and real dynamic-tool update tests prove these native mutation paths;31 Agent tests prove setter effects on subsequent requests.

External mutation of a returned owned snapshot is not an implicit write API; approved owned-value rule applies. Signal-only fields now carry AbortSignal; active run owns AbortController. Latest focused carrier batch in `/tmp/pi-rust-repin-delta-agent-core/core-abort-behavior.log` keeps Agent31/loop43/harness7/proxy13 plus iterator/frozen matrices GREEN.
