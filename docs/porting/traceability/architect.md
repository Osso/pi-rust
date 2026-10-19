# coding-agent architect traceability

Pinned source: `packages/coding-agent/src/architect/*.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/architect/` (`prompt.rs`, `observer.rs`, `project_spec.rs`, `main.rs`).
Focused tests: `crates/pi-coding-agent/tests/architect.rs` (15).

## prompt.ts

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `ARCHITECT_SYSTEM_PROMPT` | exact system prompt | `ARCHITECT_SYSTEM_PROMPT` |
| `buildArchitectPrompt` | three instruction lines + compact observation JSON in `{reason, requests, sessions}` order, `\n\n` joined | `build_architect_prompt`; `prompt_serializes_the_observation_after_the_bounded_instructions` + differential `differential_architect_prompt` |

## observer.ts

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `ARCHITECT_SESSION_ID`, `ArchitectSessionSnapshot`, `ArchitectChannelMessage`, `ArchitectObservation` | wire shapes and key order | same-named types |
| `snapshotArchitectSessions` | copy + `localeCompare` id sort | `snapshot_architect_sessions` |
| `createArchitectObservation` | requests win; unchanged sessions → undefined | `create_architect_observation`; four observation tests |
| `readArchitectSnapshot` | read-only DB, newest metadata per pid, live health, exclude architect/subagents, limit 20 | `read_architect_snapshot`; `keeps_the_deterministic_newest_live_metadata_row_per_session_id`, `keeps_the_most_recently_registered_main_session_per_live_process`, `missing_control_database_returns_empty_without_creating_it` |
| `ArchitectObserver` | per-observer claim token, snapshot reader injection, claim/renew/complete | `ArchitectObserver`; `observer_reads_a_snapshot_on_every_observation` |

## project-spec.ts

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `readProjectSpec` | relative `.md` inside `docs/specs`, canonical containment | `read_project_spec`; `project_spec_reads_only_markdown_inside_docs_specs` |
| `findProjectRoot` | nearest ancestor with `docs/specs/README.md`, exact error | `find_project_root`; `project_spec_reports_a_missing_spec_index` |
| `registerArchitectProjectSpecTool` | tool registration | **blocked** on the Rust extension API |

## main.ts

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `OBSERVER_INTERVAL_MS`, `REQUEST_CLAIM_RENEW_INTERVAL_MS`, `ARCHITECT_SESSION_ID`, `ARCHITECT_RULES_SCOPE`, `ARCHITECT_EXCLUDED_TOOL_NAMES` | exact constants | same-named constants |
| `ArchitectConsolePromptQueue` | FIFO + waiter wake + interval wait | `ArchitectConsolePromptQueue`; `console_prompt_queue_is_fifo_and_wakes_waiters` |
| `createArchitectConsoleSnapshot` | service/session/cwd/generation/branch | `create_architect_console_snapshot`; `console_snapshot_and_settings_match_the_service_contract` |
| `createArchitectSettingsManager` | in-memory read-only sandbox | `create_architect_settings_manager`; same test |
| `createArchitectMultiAgentStore` | metadata attachment, archive, store restore | `create_architect_multi_agent_store` |
| `completeSentArchitectRequest` | `architect-request:` thread id, safe integer ≥ 1, main target | `complete_sent_architect_request`; `completes_only_targeted_architect_request_threads` |
| `blockArchitectGlobalBroadcast` | channel_post/list_sessions/broadcast reasons | `block_architect_global_broadcast`; `blocks_global_broadcast_tools_for_the_architect` |
| `runArchitectCycle` | skip without observation, renew timer around prompt | `run_architect_cycle`; `architect_cycle_prompts_only_when_an_observation_exists` |
| `createArchitectStopHandler` | idempotent stop, abort, 5s force exit | `create_architect_stop_handler` |
| `waitForArchitectInterval` | 30s or abort | `wait_for_architect_interval` |
| `runArchitectServiceLoop` | console lifecycle + cycles + prompt drain | `run_architect_service_loop` |
| `createArchitectConsoleServer`, `runArchitectService` | resident console + session creation | **blocked** on `AgentSessionEvent` and `createAgentSession` |

## Gaps

- `createArchitectConsoleServer` needs the `AgentSessionEvent` type from the agent-session slice; the queue and
  snapshot contracts it wraps are ported.
- `runArchitectService` needs `createAgentSession`, `ModelRuntime`, the mailbox extension, and the bwrap extension.

## Discrepancies

None.
