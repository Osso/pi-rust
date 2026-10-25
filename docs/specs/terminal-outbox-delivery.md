# Terminal outbox projection delivery

The frozen terminal-outbox-delivery module claims persisted terminal projections, publishes agent snapshots, acknowledges delivery and runs artifact cleanup. [Current proof/dependencies](../porting/traceability/terminal-outbox-delivery.md).

## What it must do

- [x] Preserve f64 cleanup cadence and shared Date/toISOString retention thresholds, including fractional and invalid/nonfinite numbers.
- [x] Skip the clock when no persistence target exists; preserve throwable clock Error/non-Error values and invalid-date errors before claiming.
- [x] Keep registered commands before terminal coordination; normal prompt coordination must not mutate outbox attempts before agent_start.
- [x] Prove reciprocal TS/native terminal-row claim, publication, acknowledgement, retry/poison, original-error propagation and cleanup-finally behavior.
- [x] Remove the temporary reserved-finalize native bool/error sink after the stage4 Result listener migration; apply frozen's store-local listener catch/log.

## How it works

- [Source branch/caller mapping](../porting/traceability/terminal-outbox-delivery.md).

## Implementation inventory

- `crates/pi-coding-agent-storage/src/core/terminal_outbox_delivery.rs`: delivery and pure helpers.
- `crates/pi-coding-agent-storage/src/core/detached_job_cleanup.rs`: cleanup clock carrier.
- `crates/pi-coding-agent-core/src/core/agent_session.rs`: direct callers and catch boundary.
- `crates/pi-coding-agent-extensions/src/bundled_extensions/agents_core/`: mechanical runtime callers.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/repin_storage_terminal_outbox.rs`
- `crates/pi-coding-agent/tests/repin_storage_terminal_matrix.rs` (module in `suite_session_1`) — real reciprocal terminal rows, claim conflict, retries/poison and artifact deletion.
- `crates/pi-coding-agent/tests/terminal_outbox_delivery.rs`
- `crates/pi-coding-agent/tests/agent_session.rs::registered_commands_precede_terminal_outbox_drain_and_normal_prompt_coordinates_afterward`

## Known gaps (current cycle)

The current realistic-branch matrix passes. Whole control/store integration acceptance remains separate.

## Out of scope

Store/control-DB whole-module acceptance and live-state operations.
