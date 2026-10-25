# Traceability: `packages/agent-core/src/agent.ts`

Reference: immutable `4f9e6f8a4`. Rust: `crates/pi-agent-core/src/agent/agent.rs`.
Evidence and exclusions: [slice-1 proof ledger](agent-core-api-repin.md). Complete with approved owned-value/event-snapshot and typed-input exclusions. Live-state mutation by frozen built-ins is explicitly mapped to native setters below.

| Frozen surface/function | Rust | Proof |
|---|---|---|
| QueueMode, AgentOptions, Agent | re-exported QueueMode, AgentOptions, Agent | construction/state/options tests |
| defaultConvertToLlm, DEFAULT_MODEL, EMPTY_USAGE, createMutableAgentState | default_convert_to_llm, default_model, owned initial state, failure message builder | default/custom state and unsupported-default-provider tests |
| PendingMessageQueue enqueue/hasItems/drain/clear | PendingMessageQueue equivalents | all/one-at-a-time queue tests |
| constructor provider callbacks and mutable public configuration | explicit on_payload/on_response/on_retry; Rust setters for provider functions, hooks, budgets, retry cap, transport/session/tool mode | frozen callback/config test; existing forwarding tests |
| state, steeringMode, followUpMode | owned snapshots/accessors, queue setters; Agent-owned reference metadata for frozen built-ins | state/queue tests; reference proof below |
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

## Registered acquisition rejection (2026-10-24 prerequisite)

Frozen agent-loop.ts:488–494 does not stringify acquisition throws. Default Agent.runWithLifecycle catches at agent.ts:515–520; handleRunFailure:524–548 uses Error.message/String(error), and finally calls finishRun even if that conversion rejects. New actual-source `model_registry_agent_stream_throw` compares complete event/message/state values with independently clock-bounded timestamp masking. RED q15e-default-agent-stream-throw-red-3: raw lone-unit text and String-conversion rejection differ from native's empty-message projection; TypeError control passes. Native emit_run_failure now performs the source catch conversion from the retained CallbackError; finish_run_with always clears the run after the conversion/event Result settles, including conversion rejection. No StreamFn injection is used. GREEN pending.

The removed process-global get/setDefaultStreamFn API has no frozen counterpart. Default streaming calls pi-ai/compat directly. Rust support types (PromptInput, ContinueOptions, AgentInitialState, listener/hook aliases) express TS overloads/inline structural types; they do not add old runtime behavior.

## Frozen reference identity

Contract: [spec](../../specs/agent-reference-identity.md). Frozen `agent.ts:82–103` copies top-level arrays without cloning their objects; processEvents581–583 stores event.message itself before awaited listener dispatch. Coding-agent `agent-session.ts:2810–2824` compares model/message references, not values.

| Requirement | Native authority | Proof |
|---|---|---|
| Same-value model/message replacement rejects an old snapshot | AgentStorage model generation + opaque Arc token per message under the same mutex | `pi-agent-core/tests/agent_identity.rs::reference_identity_matches_frozen_same_value_replacement_and_preserved_prefixes`, actual pinned Agent |
| Append/prefix copy/truncate retain surviving references; reset does not resurrect them | append_message/truncate_messages, fresh replacement tokens | Same differential, including supervisor-status exclusion and unrelated system/tools changes |
| Earlier listener appends while the event still names its original message | Event-scoped token from the authoritative append | `earlier_listener_append_does_not_change_dispatched_message_token`, actual frozen awaited listener ordering |
| Getter has no stale/foreign/task-external value | Tokio task-local scope around the entire listener invocation and awaited future | Same native test: synchronous callback access succeeds; before/after, another Agent and spawned task fail explicitly |
| Built-in custom/bash appends preserve prefix identity | Source pushes use append_message, retry/overflow slices use truncate_messages | `pi-coding-agent/tests/agent_identity_paths.rs`; concrete custom append, real bash append, truncation and same-value replacement |

This is literal reference identity required by a frozen built-in, not an approved semantic deviation. Public state/event JSON is unchanged. Advisory/validation execution is still pending its cancellation/lifecycle slice.

## Built-in live-state consumer audit

Rebuilt/restored transcripts from coding-agent SDK/runtime and session materialization use `set_messages` with fresh object tokens. Frozen agent-session.ts direct custom/bash pushes3942,5860,5915 map to `append_message`; prefix slices2037,4852,5121,5683,5729 map to `truncate_messages`, preserving surviving identities rather than rebuilding equal-valued messages. Frozen tools assignments (2476,2858,2869), systemPrompt assignments (2480,3178,3182,5222), model assignments (4150,5276,5687) and thinkingLevel assignment4239 map to explicit native setters and active-tool override methods. SDK restore, agent-session compaction/branch/generation and real dynamic-tool update tests prove these native mutation paths;31 Agent tests prove setter effects on subsequent requests.

External mutation of a returned owned snapshot is not an implicit write API; approved owned-value rule applies. Signal-only fields now carry AbortSignal; active run owns AbortController. Latest focused carrier batch in `/tmp/pi-rust-repin-delta-agent-core/core-abort-behavior.log` keeps Agent31/loop43/harness7/proxy13 plus iterator/frozen matrices GREEN.
