# Orchestrator supervisor
Frozen `packages/orchestrator/src/supervisor.ts` at [common pin](../source-lock.json).

| Surface/branches | Proof |
| --- | --- |
| OrchestratorSupervisor, singleton-equivalent instance per runtime, handler dispatch | `tests/supervisor.rs` |
| spawn/list/status/RPC/stop; persisted session metadata; summary optional keys | `supervisor_handler_stream_lifecycle_matches_frozen` differential with real fake child |
| openRpcStream command/response/event/UI forwarding and close | same frozen differential; raw supervisor response not sent to event subscribers behavioral regression |
| recoverAfterRestart online/starting→stopped, error unchanged; unknown instance four dispatch branches | `restart_recovery_and_handler_unknown_instance_contract` |
| unexpected exit/failSpawn/shutdown/metadata refresh command set/multiple subscribers | implemented; additional failure fixtures pending |

Native runtime creates one shared supervisor and Radius coordinator, with weak coordinator binding to avoid an ownership cycle. Source maps use insertion-ordered IndexMap. Initial-ID/time injection is boundary-only; frozen differential normalizes generated instance UUID and fixes clock without modifying source bytes.
