# Supervisor/runtime frozen re-pin (Q16)

Reference: [common source lock](../porting/source-lock.json), `4f9e6f8a4`.

Supervisor request prompts reconcile cumulative objectives against explicit user instructions and persistent contracts. Assistant plans, prior reviews, summaries and repeated objectives do not authorize constraints. Goal-set and goal-progress requests preserve grounded parent requirements, exclusions and completion criteria, remove unsupported additions, and permit scope narrowing only by explicit user instruction. Goal-progress responses avoid routine task decomposition.

The resident regex delta remains governed by [delta lock](../porting/delta-lock.json). Q14 standalone `--no-supervisor` behavior is outside this slice.

Restart handoff rejects an empty session path and clears provider/model/thinking CLI overrides so saved session selection wins. Empty text content in a structured Supervisor response does not mask the object response; whitespace-only text still parses as invalid JSON.

Resident console probes validate protocol/service without replacing the attached owner or subscribing to events. Snapshots preserve optional identity with version, PID, executable, optional entrypoint, instance ID, manager (`pi`/`external`), and `ready: true`. Malformed identity is rejected.

Supervisor evaluation waits use the frozen 300ms default. Cancellation takes precedence over expiry and preemption, signals the evaluator, waits for its settlement, and never overwrites cancelled rows. Evaluation failures propagate to the caller rather than completing the row inside the service.

Instruction classification is stateless, exact-token only (no trimming/case normalization); its context contains only the veto system prompt and current instruction text/timestamp. Runtime identities are superseded only for equal PID/start marker and two present, unequal incarnations.

Supervisor startup probes version, coalesces local startup by resolved DB path, holds the cross-process proper-lockfile start lock, and reprobes before launching. Matching residents are reused; external incompatible residents fail with service-manager guidance; Pi-managed incompatible residents are terminated and their instance exit awaited before replacement. Launch cwd is home, environment marks autostart, script/compiled invocation is preserved. Readiness/exit have bounded polling deadlines. External `setsid` is the approved mandatory detached-launch mechanism.

Supervisor clients ensure startup before computing request deadlines/posting rows. Startup errors return bounded error responses. Pre-abort never starts/posts; abort during polling or retry wakes promptly, sender-fenced cancels any posted request, and notifies the wake socket only when cancellation changed the row. Cancelled rows return stored error reasons. Default polling is 300ms.

Automatic idle/completion reviews pass non-generic continue instructions through a stateless veto. Accept preserves instructions; rejection, invalid decision or evaluation failure substitutes the generic continuation and appends hidden policy feedback. Cancellation propagates rather than becoming veto feedback. Generic continuation and other response/request kinds bypass the veto.

Restart recovery appends a result for every unanswered call in the restored pending assistant batch: detached jobs point to background notifications, restart_self succeeds, and other calls report interruption. Already answered calls are not repeated; results precede the restart notice.

Supervisor and Architect session opening use resident-file preparation, retaining a live listener or newest dated resident file and deleting stale candidates/metadata before opening. The approved private-regex delta is unchanged.

Supervisor requires gpt-6.1-sol on an authenticated provider in frozen order: openai-codex, openai-codex-gc, openai-codex-team. The KB-only resource loader registers no old remote-compaction extension. Supervisor snapshots advertise version/process identity, stable per-process instance UUID, readiness, and Pi/external management based on autostart environment.

Project identity extraction applies JS trimming and does not strip a scheme authority unless its terminating slash exists. Existing unreadable project config propagates a read error; only absent config returns empty mappings. Architect spec reads canonicalize the project root first and reject every relative path starting with `..`, including dotdot-prefixed filenames, exactly like frozen TS.

Supervisor request/console processing propagates failures instead of dropping them or logging-and-continuing. Cancellation still suppresses completion-claim failures only when the stored row is cancelled.

Evaluation cancellation listens across compaction, prompt execution and instruction review, not just while prompting. It aborts the session and then awaits evaluator settlement; cancelled compaction must not start a new prompt.

Architect specs use Node-compatible UTF-8 decoding (replacement characters for malformed bytes), including index discovery. Realpath/read failures retain Node syscall/path messages. Absolute project paths do not consult cwd, and relative paths never fall back to `/` on cwd failure.

Self-restart parent waiting treats every failed signal-zero probe, including EPERM, as not alive, matching frozen defaultIsProcessAlive rather than the separate runtime-process liveness policy.
