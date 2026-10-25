# coding-agent architect traceability

Pinned source: `packages/coding-agent/src/architect/*.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/architect/` (`prompt.rs`, `observer.rs`, `project_spec.rs`, `main.rs`).
Focused tests: `crates/pi-coding-agent/tests/architect.rs` (18), `tests/differential_architect_request_loop.rs` (1). Main-module successor audit below uses the common pin; historical mappings for the companion modules remain above.

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
| `registerArchitectProjectSpecTool` | tool registration | `register_architect_project_spec_tool` / `architect_project_spec_tool_definition`; `project_spec_tool_registration_reads_specs_and_reports_paths` |

## main.ts

Whole-module common-pin audit: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, 309 lines, completed 2026-10-25 under the lighter-proof rule.

| TypeScript source | Contract | Rust and proof |
|---|---|---|
| `OBSERVER_INTERVAL_MS`, `REQUEST_CLAIM_RENEW_INTERVAL_MS`, `ARCHITECT_SESSION_ID`, `ARCHITECT_RULES_SCOPE`, `ARCHITECT_EXCLUDED_TOOL_NAMES` | exact constants | same-named constants |
| `ArchitectConsolePromptQueue` | FIFO + waiter wake + interval wait | `ArchitectConsolePromptQueue`; `console_prompt_queue_is_fifo_and_wakes_waiters` |
| `createArchitectConsoleSnapshot` | service/session/cwd/generation/branch | `create_architect_console_snapshot`; `console_snapshot_and_settings_match_the_service_contract` |
| `createArchitectSettingsManager` | in-memory read-only sandbox | `create_architect_settings_manager`; same test |
| `createArchitectMultiAgentStore` | metadata attachment, archive, store restore | `create_architect_multi_agent_store` |
| `completeSentArchitectRequest` | `architect-request:` thread id, safe integer ≥ 1, main target | `complete_sent_architect_request`; `completes_only_targeted_architect_request_threads` |
| `blockArchitectGlobalBroadcast` | channel_post/list_sessions/broadcast reasons | `block_architect_global_broadcast`; `blocks_global_broadcast_tools_for_the_architect` |
| `runArchitectCycle` | skip without observation, renew every 30s while prompting, cancel renewal on completion/error | `run_architect_cycle`; `architect_cycle_prompts_only_when_an_observation_exists`; native renewal failure RED→GREEN |
| `createArchitectStopHandler` | idempotent stop, abort, 5s force exit | `create_architect_stop_handler` |
| `waitForArchitectInterval` | 30s or abort | `wait_for_architect_interval` |
| `runArchitectServiceLoop` | console lifecycle + cycles + prompt drain | `run_architect_service_loop` |
| `createArchitectConsoleServer` | resident console host: snapshot, prompt queue, session subscription | `create_architect_console_server`; `console_server_exposes_the_architect_snapshot_and_prompt_queue` |
| `openArchitectSession` | prepare the resident file, open existing or create fixed-id session | `open_architect_session`; shared prepared-resident/session-manager proofs |
| `runArchitectService` | HOME nullish cwd, resident open, auth.json + models.json registry, exact sol lookup/error, mailbox callbacks + bwrap + spec/gate/prompt extension, observer role/rules/excluded tools/high thinking, console and signal lifecycle | `run_architect_service`; whole wiring audit plus existing console-server/request-cycle tests |

## Successor proof

Audited the entire main module, including FIFO prompt drain/abort ordering, queue wake/interval behavior, metadata attachment/archive/store restore, Number/safe-integer request completion, exact global-tool block reasons, stop idempotence and 5s force exit. The parent now supplies the frozen AuthStorage/ModelRegistry SDK service wiring; the former ModelRuntime blocker is gone. Current `AgentSession::abort` has no Err branch, so the stop callback cannot lose a returned failure on the frozen runtime path.

Fixed a silent renewal failure: a request-renewal error now returns through the existing cycle/service Result instead of being discarded in a spawned task. The pending prompt and timer end with the failed cycle. Native `architect_cycle_reports_request_renewal_failure_during_prompt` reproduces the real 30s boundary and retains the raw UTF-16 error. Frozen timer exceptions are fatal to the service; no retry or fallback is added.

Proof logs `/tmp/runtime-successor-{architect-final,architect-cycle-final,architect-lossless}.log`: 18 native tests PASS; existing frozen request-cycle differential PASS; four lossless architect tests PASS. Existing frozen prompt differential also passed (`architect-cycle.log`). No live provider or new differential/matrix.

- GAP: malformed/adversarial inputs, timer/numeric extremes, exceptional signal-registration/OS faults and Node scheduling/handler-runtime oddities use the approved input/runtime gap; exact UTF-16 carriers remain.
- Live provider turns are not exercised; source wiring is audited and existing isolated SDK/console/request-cycle proof is used, per lighter proof.

## Discrepancies

- DISCREPANCY (preserved frozen catalog bug): the default tracked model catalog has no `openai-codex/gpt-5.6-sol`, so default service startup rejects with `Pi Architect requires openai-codex/gpt-5.6-sol`. A user models.json can supply it. No model fallback or catalog supplementation is added.
