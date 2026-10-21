# Slice 1: agent core API repin

Reference: `4f9e6f8a4`, immutable `packages/agent-core/src`.

| Frozen behavior | Rust | Evidence |
|---|---|---|
| restoredPendingToolMessage exported; ignore error/aborted assistants | agent_loop::restored_pending_tool_message, re-exported at root | RED `does_not_resume_tools_from_failed_restored_assistants`; GREEN pending |
| handleRunFailure preserves partial, no second start | Agent::finish_run_with/build_failure_message | RED `preserves_streamed_partial_on_abort_without_duplicate_start`; GREEN pending |
| reset permitted during active run | Agent::reset | RED `reset_clears_state_during_active_run_without_aborting_it`; GREEN pending |
| proxy toolcall_end uses reconstructed call | process_toolcall_end, ProxyAssistantMessageEvent | RED `reconstructs_tool_call_from_deltas_without_trusting_end_payload`; GREEN pending |

RED logs: `/tmp/pi-rust-repin-agent-core-api/red-{initial,agent_loop,proxy}.log`.
Existing module docs describe old-pin proofs until explicitly revalidated. Remaining function-by-function comparison, API cleanup, targeted tests, mutants and workspace check are pending. Do not interpret this document as full parity proof.
