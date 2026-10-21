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

Approved frozen discrepancies (parent 2026-10-21): duplicate in-flight ID retains the original pending resolver even after disposal; Rust orphan sender list releases only with instance lifetime. Malformed child JSON propagates through native fatal channel to serve Err/CLI exit 1, with ordinary server/supervisor/Radius/socket cleanup, not a new immediate send rejection. Real-child RED/GREEN and frozen subprocess differential fixtures cover both.

Frozen Set identity is preserved for reused callback objects: `repeated_callback_identity_matches_frozen_set_subscriptions` and `shared_stream_callback_identity_and_close_match_frozen_sets` fail the draft token-per-registration behavior first, then compare event/exit/UI counts and close behavior to actual frozen processes. Arc callback identity deduplicates registrations; closing a stream clears the same UI callback even if reused by another stream, matching frozen equality semantics.
