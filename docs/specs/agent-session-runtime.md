# Agent session runtime

Port the whole frozen `core/agent-session-runtime.ts` at the common pin. [Source branches, migration and proof](../porting/traceability/agent-session-runtime.md).

## What it must do

- [x] A prepared tool-result cwd relocation prevents a later session replacement from starting until activation releases the lifecycle transition.
- [ ] Serialize all lifecycle transitions in frozen FIFO order, release after failure, and release prepared relocation before continuing the replacement session.
- [ ] Invoke before-invalidate, before-process-restart and rebind hooks at their frozen boundaries; interactive mode uses that authority instead of partial session adoption.
- [ ] Match reload, working-directory recovery, archived resume/trust propagation, restart cleanup and creation/teardown side effects.
- [x] Teardown emits shutdown, invokes beforeInvalidate, removes an abandoned empty session, then disposes that captured session. Do not pre-abort or add queue_update events; failures stop subsequent steps. Real frozen/native queue-event RED drives this correction; bounded GREEN/final acceptance follows implementation.
- [x] 4b recovery: beforeRun on idle continue, prompt and registered commands; replacements forward to the new session, releasing the original prompt permit. afterTurn runs before unchanged restriction/prepared-relocation/duplicate/previous stop checks. Runtime repairs process cwd, emits the frozen custom notice and relocates before run or prepares after turn. Callback throws retain raw values/native properties through AgentSessionError. Node cached previous-path notices follow the approved [native cwd gap](process-cwd.md). Genuine continue RED→GREEN, source/native prompt/command/streaming/error cases, real-turn activation and isolated physical-cwd visible-notice proof are recorded in the [4b matrix](../porting/traceability/agent-session-runtime.md#4b-recovery-checkpoint-2026-10-23). Broader runtime4c/4d remains open.
- [ ] Preserve existing standalone/noSupervisor rejection boundaries, common-pin fork behavior and native CallbackError projections during re-port.

## How it works

- [Runtime traceability](../porting/traceability/agent-session-runtime.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/agent_session_runtime.rs` — current session and lifecycle authority.
- `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs` — native UI lifecycle consumer.
- `crates/pi-coding-agent/src/modes/{extension_actions,print_mode,rpc/rpc_mode}.rs` — non-interactive runtime consumers.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/agent_session_runtime.rs` — real runtime lifecycle and replacement behavior.
- `oracle/agent-session-runtime-gate-oracle.mts` — actual frozen prepare/new/activate gate differential.
- `oracle/agent-session-runtime-rebind-oracle.mts` — held rebind and worker/owner FIFO characterization.
- `ui_tests::reload_body_matches_frozen_loading_settings_status_and_raw_failure` and `oracle/interactive-reload-body-oracle.mts` — bounded interactive runtime reload body success/raw-failure proof; pending branches remain explicit in traceability.

## Known gaps (current cycle)

- [ ] Whole-module branch matrix and proof remain in progress; historical tests alone are not common-pin acceptance.
- [x] Explicit UI-thread hook dispatch and removal of partial `adopt_session`; worker activation and interleaved worker/owner rebind ordering proven.
- [ ] Individual reset/restart UI side effects and full resource binding/reload branch proofs remain pending.

## Out of scope

JavaScript shared-reference identity follows the project approval except where frozen built-in consumers depend on mutation. Existing approved platform gaps remain unchanged.
