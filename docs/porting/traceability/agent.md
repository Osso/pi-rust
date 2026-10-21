# Traceability: `packages/agent-core/src/agent.ts`

Reference: immutable `4f9e6f8a4`. Rust: `crates/pi-agent-core/src/agent/agent.rs`.
Evidence and exclusions: [slice-1 proof ledger](agent-core-api-repin.md). Status todo pending owned-reference classification; no whole-module parity claim.

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
| handleRunFailure | build_failure_message and failure event sequence | preserve thinking/text/tool call/usage/identity on steer/abort/stream failure; one assistant start |
| processEvents | process_events | state reduction, truthy error text, awaited listeners; log rejection and continue |
| prepareNextTurn wrappers | build_prepare_next_turn_hook | context-hook precedence and dynamic replacement after turn listeners |

The removed process-global get/setDefaultStreamFn API has no frozen counterpart. Default streaming calls pi-ai/compat directly. Rust support types (PromptInput, ContinueOptions, AgentInitialState, listener/hook aliases) express TS overloads/inline structural types; they do not add old runtime behavior.
