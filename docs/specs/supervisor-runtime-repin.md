# Supervisor/runtime frozen re-pin (Q16)

Reference: [common source lock](../porting/source-lock.json), `4f9e6f8a4`.

Supervisor request prompts reconcile cumulative objectives against explicit user instructions and persistent contracts. Assistant plans, prior reviews, summaries and repeated objectives do not authorize constraints. Goal-set and goal-progress requests preserve grounded parent requirements, exclusions and completion criteria, remove unsupported additions, and permit scope narrowing only by explicit user instruction. Goal-progress responses avoid routine task decomposition.

The resident regex delta remains governed by [delta lock](../porting/delta-lock.json). Q14 standalone `--no-supervisor` behavior is outside this slice.

Restart handoff rejects an empty session path and clears provider/model/thinking CLI overrides so saved session selection wins. Empty text content in a structured Supervisor response does not mask the object response; whitespace-only text still parses as invalid JSON.

Resident console probes validate protocol/service without replacing the attached owner or subscribing to events. Snapshots preserve optional identity with version, PID, executable, optional entrypoint, instance ID, manager (`pi`/`external`), and `ready: true`. Malformed identity is rejected.

Supervisor evaluation waits use the frozen 300ms default. Cancellation takes precedence over expiry and preemption, signals the evaluator, waits for its settlement, and never overwrites cancelled rows. Evaluation failures propagate to the caller rather than completing the row inside the service.
