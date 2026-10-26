# Agents-core runtime — frozen 4f9e6f8a4

Source: `packages/coding-agent/extensions/agents-core/src/runtime.ts`, 3,326 lines at [the common pin](../source-lock.json). Native implementation: `crates/pi-coding-agent-extensions/src/bundled_extensions/agents_core/`. Contract: [agents-core runtime](../../specs/agents-core-runtime.md).

Whole-module audit completed 2026-10-25 under the approved lighter-proof rule. The runtime row is **done** within the user-observable parity scope. No realistic branch remains open in the factory, browser, registry, archive, artifact-root, cancellation-timeout or recovery audit. Non-persistence closure is source reading plus existing tests; the [control-DB reciprocal proof](session-control-db.md) and [runtime mailbox acceptance proof](runtime-mailbox-acceptance.md) remain separate, unchanged authorities. This is not a workspace-wide acceptance claim.

## Reciprocal extension persistence (2026-10-26)

| Boundary | Behavioral proof |
| --- | --- |
| Frozen extension → native extension → frozen extension, and reverse | `agents_core_runtime::reciprocal_agents_core_ts_rust_ts_extension_persistence` and `reciprocal_agents_core_rust_ts_rust_extension_persistence`; frozen adapter: `oracle/agents-core-repin-oracle.ts`, `reciprocal` operation |

Each direction uses one private state directory and the same control DB/session paths throughout. The first implementation attaches an existing saved session through the extension API. The peer reopens the persisted store and sends a Unicode message through the extension mailbox API. The first implementation reopens and dispatches a scripted child to completion. After every mutation, the peer reopens the same state and compares **all agent snapshots, mailbox records, full list/view tool content and details**, without ID/time/path normalization. Both implementations also return the exact same terminal `waitNotifications` wake from the persisted completion. Assertions require a waiting attachment, a pending addressed message, two distinct persisted records and the completed child's concrete summary. The attached session file remains byte-identical.

The frozen side registers the actual bundled extension and invokes its tools; the native side uses the public agents-core request handler, mailbox, viewer and wait APIs. Session-manager bootstrap creates the saved-session fixture; no raw DB seed or JSON state transfer substitutes for extension operations. The child factory is scripted, without a provider or network. This proves extension-owned attachment, message and terminal lifecycle persistence, including the persisted terminal notification read boundary. It does **not** claim a reciprocal production-child session-file writer, wait-tool notification acceptance/claim consumption, live ownership transfer, or close of an unowned attachment. Those remain under the existing factory, mailbox acceptance and runtime tests.

Focused command: `cargo test -j3 -p pi-coding-agent --test suite_core_1 reciprocal_agents_core -- --nocapture`. No production discrepancy was found; no production code changed.

## Source map

Line ranges below refer only to the frozen runtime. Native split filenames are implementation organization, not additional frozen source modules.

### Contracts, schemas and helpers (1–455)

- Schemas, parameter/detail records, factory contracts, owned runtimes, handles and notification registrations map to `shared.rs`, `runtime.rs`, `session_factories.rs`, `spawn_attach_runtime.rs`, `control_runtime.rs`, `registration.rs` and `dispatch_runtime.rs`. Existing registration differential checks the full tool/command metadata and ordering, including set_agent_model and question relay registration.
- `createMultiAgentRuntimeHandles`, `resolveSelectedSessionMutationTarget` and `artifactRootForStore` map to `runtime.rs`; live targets delegate to the bound child mutation surface. [Model-tool proof](agent-model-tool.md) covers inactive, detached and missing/live target selection. Artifact roots are held per store with weak ownership and registered by either production factory; terminal publication and wait reconciliation use that root, not the session directory.
- `result`, `emptyResult`, `errorResult`, `emptyAgent` and the three empty-message constructors map to `tool_results.rs`, `shared.rs`, `runtime_utils.rs`, `control_runtime.rs` and `mailbox_runtime.rs`. errorResult deliberately omits isError; explicit route errors retain their separate flag. Epoch timestamps and empty IDs match frozen literals.
- `validateGoalObjective`, production-factory detection and prompt diagnostics map to `session_factories.rs`: JS trim, 10,000 UTF-16 units, empty/oversize messages, production-only public validation. Rust uses the factory's production marker rather than a JS Symbol. Human text and argument JSON retain JsString/JsValue.
- `DEFAULT_BROWSER_AGENT_CONFIG_ROOT` is the import-time HOME snapshot. Native `default_browser_agent_config_root` is initialized before CLI dispatch; the existing HOME/NSS subprocess proof checks both the visible failure and immutable snapshot. It is not re-probed for each factory.

### Background jobs (414–580)

`notifyBackgroundDispatch`, `startBackgroundDispatch`, `backgroundCommand` and `jobsCommand` map to `registration.rs` and `runtime::jobs_command`.

The audit covers child-runtime rejection; missing factory; production validation; missing persistence; prepare→construct→failed/running commit; transcript forwarding; disposal on rejected commit; publication before dispatch; ownership release; editor clearing; initial and terminal UI notifications; no-jobs and ordered job listing. Notification UI is acquired when the dispatch settles, not when it starts. Construction failure messages preserve unpaired UTF-16 exactly. Publication/commit/async-notification errors do not disappear.

Proof: `background_command_reports_unavailable_and_validation_errors`, `jobs_command_lists_background_agents`, `background_construction_failure_preserves_frozen_utf16_notification`, and the subprocess-backed `public_background_completion_uses_live_ui_and_reports_async_getter_failure`.

### Child and attached factories (582–869)

`resolveChildExtensionFactories`, browser read tools, transcript metadata, active-tool lookup, unresolved-turn ancestry, inherited-leaf selection, manager construction, both exported production factories, profile/model/tool resolution and configured-profile validation map to `session_factories.rs` and `spawn_attach_runtime.rs`.

- Fresh/inherited manager creation preserves parent identity, session directory, subagent name and fork event. Active tool identity is checked even for fresh context; inherited context excludes the unresolved turn and rejects missing parent persistence/tool entries.
- Both factories forward the parent model registry, selected model/effort, child identity/store/runtime role and extension factories. Goal factories are removed; static/dynamic factories retain order. Restricted profiles append ask_questions/contact_parent/send_agent_message/end_turn, plus read for browser. Supervisor/child-disabled tools and change_working_directory are excluded.
- Browser factories install the actual restricted read definition with the supplied root or import-time default. Existing factory proof executes reads both inside and outside a root distinct from cwd, for fresh and attached sessions.
- Attached factories restore archives before existence/identity validation, validate cwd/session ID, set the metadata DB, bind extensions and expose the unchanged transcript. SDK owns settings creation: no old attached policy override remains. Child managers persist for recovery before the bound wrapper is returned.
- Spawn metadata inherits the configured or parent model, defaults effort to medium independently of parent effort and clamps to model capabilities. Attachment metadata retains configured effort without that spawn clamp. Explicit unknown profiles fail before persistence admission with sorted configured-key guidance. Raw profile access does not deserialize unrelated fields.

Proof: all 19 `agents_core_session_factories` tests; accessed-model/context/getter cases in `agents_core_drift`; real faux-provider child in `agents_core_end_to_end`. Bound session delegation/disposal and previous-message identity remain in [child-session](child-session.md) and [child-response](child-response.md).

### Pi bridge and summaries (871–1074)

`createMultiAgentPiRequestHandler`, current/select helpers, wait-argument validation, child/supervisor predicates, main snapshot and last-message helpers map to `runtime.rs`.

The audit covers disposal; mirror binding; child orchestration guards; spawn/wait/attach/list/current/select/messages.last/messages.send; null/absent list parameters; callback true/false/undefined selection; main selection; no-message null; string/block/custom content; newline joining; absent/empty text; exact 2,000-unit clipping. The native slice retains a split surrogate. Bridge waits release mailbox claims because bridge results are not accepted session input; bridge send does not run the model-tool-only callback.

Proof: request-handler runtime tests, wait receipt/bridge-release test, tool-only callback-scope test, empty-parent list differential and existing lossless runtime tests. SelectAgentView is an awaited native UI bridge; no JS synchronous callback timing or shared-return-reference contract is claimed.

### Spawn, attachment and recovery (1076–1680)

`spawnAgent`, ownership release, attachment resolution/admission/dispatch/reservation, recovery ordering/live-owner/dead-owner paths and reserved-child construction map to `spawn_attach_runtime.rs` and `dispatch_runtime.rs`.

- Spawn preserves validation/profile/persistence ordering, configured context, permission and model metadata; passes the original prompt and active tool ID; commits failed construction or running transcript identity; disposes rejected children; publishes before dispatch and releases ownership afterward. No parent journal or transcript requirement is added to a custom factory.
- Attachment accepts exactly one trimmed path/id/name selector; restores archive paths; reads control-DB name/subagentName; deduplicates local/all listings by path; uses exact ID before prefix and exact name; rejects ambiguous/missing matches. It preserves session identity and parent account/model/permission, with on-request permission when parentless. Optional resume reserves ownership first; thrown reservation errors propagate, while an ordinary non-acquisition returns an attached, undispatched record.
- Recovery filters direct children only for a child runtime, orders deepest first and preserves ties, skips current dispatches/waiting agents, resumes transcript-bearing non-cancelling agents with the exact crash prompt, and otherwise checks live/superseded ownership before dead-child settlement. Missing journal entries do not block recovery.
- Waiting attachments construct before confirming runtime; restore-generation changes or rejected confirmation abort/dispose rather than prompt. Construction failure finalizes with runtime_spawn_failed; publication failures propagate.

Proof: runtime spawn/attach/recovery tests; five-vector public profile fixture; archived path/metadata fixtures; `attach_resume_propagates_runtime_reservation_failure` uses a real SQLite trigger after attachment admission and checks the error, retained waiting record and absent dispatch/factory call. It adds no storage algorithm or new oracle scenario.

### Dispatch and terminal publication (1682–1873)

`runAgentSession`, descendant settlement, finalization, cancellation acknowledgement, coordinator publication and exported terminal-outbox delivery map to `dispatch_runtime.rs` and `lifecycle_runtime.rs`.

Construction/admission, generation guards, lease-loss abort, store abort handler, live handle/transcript publication, previous-message capture, prompt, detached cancellation and descendant wait precede completion. Cancellation acknowledgement precedes completed/failed settlement. Steering-pending completion drains coordination and rejects missing/ineffective drains. Error handling settles descendants without the cancelled signal before terminalization. Lease/store registrations and live handles are removed before awaited disposal. Coordinator/transcript/terminal-outbox failures propagate through Result; asynchronous rejected dispatches reach the shared uncaught authority after cleanup.

Proof: real spawn and terminal-notification tests, transcript-failure-before-prompt test, current-turn/error/UTF-16 child-response cases, cancellation/descendant ordering and terminal-artifact-root tests. [Descendant settlement](descendant-settlement.md), [detached cancellation](detached-runtime-cancellation.md) and [terminal outbox](terminal-outbox-delivery.md) retain their own full boundary proofs.

### Listing and mailbox (1875–2136; 2635–2676; 2910–3119)

`listAgents` and formatting map to `viewer.rs`; active descendants vs all active agents, empty parent filter, singular/plural header, JSON-quoted display names and store order are retained.

Send/contact-parent/current-sender helpers, failed transport handling, mirror binding/disposal, pending lifecycle scans, store references, recipient routing and empty direct/parent messages map to `mailbox_runtime.rs`. The audit covers identity-unavailable, local parent/child, explicit session mismatch, main/supervisor, remote relay, missing transport, durable failed status, local-only model-tool callback and exact target-bearing error/success text. Transport rows refer to canonical store content, never copied bodies. Lifecycle mirroring uses frozen detached/suppressTerminalNotification gating, not an extra transport scan. Parent routing uses runtime parent identity, not old transcript headers. Observer failures return to the store's existing diagnostic boundary.

Proof: runtime canonical mailbox delivery/parent-contact/callback/route-error/empty-session/parent-header/detached-mirror tests, drift observer regression and existing deterministic registration/list cases. [Route](session-message-route.md), [mailbox-tool](mailbox-tools.md), [control DB](session-control-db.md) and [mailbox acceptance](runtime-mailbox-acceptance.md) retain frozen bidirectional persistence proof. Storage algorithms were not changed in this lane.

### Wait notifications and coordination (2138–2524)

Wait wake variants, watcher, exported wait/consume functions, coordination recipient/claim/channel helpers, terminal notification listing/consumption and in-flight predicate map to `wait_runtime.rs` and `shared.rs`.

The watcher captures active-agent order and the latest model-request deadline. Initial terminal notifications precede waiting; terminal reads precede steering/coordination/timeout wakes. Empty tracked sets, pre-abort, 3-second reconciliation/polling, terminal-first deadline ticks, error wakes, cancellation and subscription cleanup are retained. Reconciliation and terminal projections share the configured artifact root and timestamp. A timed-out slice leaves agents running.

Consumption preserves supplied wake prompts, durable mailbox claim receipts, release-on-preparation-error, channel pagination/cursor advancement and remote-origin filtering. Bridge wait releases claims; accepted tool input owns durable delivery. Terminal wakes consume completed/failed notifications in persisted order and preserve text/details; none/cancelled/error/unavailable/steering output matches frozen.

Proof: wait timeout/terminal/coordination/receipt tests, two persisted watcher tests (ordered terminal choice and error wakes), shared mailbox persistence proof and existing runtime coordination tests. No new fault/scheduling differential was added.

### Cancellation, steering and desktop notifications (2526–2633; 2678–2908)

Exported cancellation/steering/wakeup functions and private close/steer helpers map to `control_runtime.rs`; waiting notification creation/replacement/transition/closure maps to `dispatch_runtime.rs`.

Cancellation walks active descendants in reverse and suppresses detached descendant terminal messages, then handles the target. Owned runtimes request cancellation, publish, safely abort and await the shared dispatch only up to the configurable timeout; timeout does not drop the actual running dispatch, and a rejected dispatch remains an error. Non-owned runtimes use the persisted detached cancellation authority.

Steering checks existence/activity, coordinator/persistence/ownership/process/actor/session/transcript authority, then publishes the coordinator message and notifies waiters. Interactive steering uses next_model_call; tool steering uses runtime sender identity. Desktop notifications are limited to pending input-needed system threads; old handles/subscriptions are closed on replacement, transition or non-waiting dispatch completion. Notifier/close errors use the shared frozen-style console reporter.

Proof: configured 17ms timeout/rejection regression, owned cancellation and detached tree tests, steering unknown/identity tests, existing [model-tool](agent-model-tool.md) and desktop-notification callback tests. Native notifier callbacks use the Rust-native extension contract.

### Registration and shutdown (3121–3326)

Store resolution, stop-dispatch cleanup, exported register/default functions and lifecycle hooks map to `runtime.rs` and `registration.rs`. Main model_request_start records the clock; agent_end/session_start clears it. Main session_start binds the mirror then recovers. Shutdown always disposes the mirror, but reload/child shutdown does not stop shared parent dispatches. Main quit invalidates generations, aborts owned/unowned handles, closes waiting notifications and clears handles/dispatches. Tool and command registration order, exact metadata, mailbox/question/model adapters and shared store are retained; registration failures propagate immediately.

Proof: full existing registration differential; real frozen/native child/reload/main shutdown test; model-request wait-slice test; late-registration rejection test. Frozen `src/index.ts` remains the core-only factory/export facade; runtime's default registers core+viewer+mailbox. Native package entrypoints preserve that split.

## Removed source-absent surface

`parent_agent_journal.rs`, its module export, dispatch journal wrappers/constants/terminal predicate, journal writer/compaction refresh/recovery scans and four obsolete journal tests are deleted. The frozen runtime neither writes agent_start/agent_complete records nor gates spawn/recovery on them. Existing JSONL readers of historical records outside this runtime remain unchanged. The failed-mailbox constructor remains a shared implementation helper for the three current frozen empty-message literals, not an old frozen module contract.

The historical split-source tables and claims of missing browser wiring, journal admission, surrogate dropping and timer-crash behavior have been replaced by this common-pin map.

## DISCREPANCIES and standing gaps

- **DISCREPANCY — completion after stale UI:** frozen notifyBackgroundDispatch has no catch on its continuation; invalidating the runner before completion logs the lifecycle observer failure and then rejects through the process uncaught boundary. Native preserves both diagnostics; it does not silently suppress the completion error. The subprocess test asserts the observer's exact `MultiAgentStore lifecycle notification listener failed: Error: ...` line and final uncaught line. V8 stack frames remain omitted under the standing approval.
- **DISCREPANCY — cancellation reason/timeout result:** the owned source path does not forward its reason to requestCancellation and never consumes settledBeforeTimeout. Native keeps the same behavior: return the latest snapshot after settlement or timeout; do not invent terminalization or a timeout error. Detached cancellation still forwards its reason.
- **gap:** pathological/wrong-typed/exotic JSON, OS/runtime fault shapes, numeric extremes, Node/V8/libuv scheduling/identity oddities and Windows-only branches follow AGENTS.md; no new API/carrier/fallback or adversarial hunt was added. Generated unique IDs avoid the old dispatch-map promise-identity distinction on realistic paths.
- **gap:** JavaScript shared-reference mutation and promise/microtask timing are outside the approved owned-value/native-extension contract; existing lossless UTF-16 and JSON carriers remain intact. Native callbacks do not emulate JS realm objects.

## Final proof ledger

Code baseline `5f1fa82a8`; successor changes `2daca5441` (remove old journal), `6a8c5ade5` (observer diagnostic) and `795776802` (attachment reservation propagation). Parent `1e9e576a7` merged as `e97b6f691`. Final documentation does not invalidate these commands' code scope.

| Command / exact scope | Result / artifact |
| --- | --- |
| `cargo test -j3 -p pi-coding-agent --test suite_core_1 background_construction_failure_preserves_frozen_utf16_notification -- --nocapture` | PASS, 1; `/tmp/agents-core-3-background-green.log`; pending handoff UTF-16 fix now GREEN |
| `cargo test -j3 -p pi-coding-agent --test suite_core_1 agents_core -- --nocapture` | PASS, 52 after both corrections; `/tmp/agents-core-3-runtime-final.log`; includes existing deterministic/registration proof; supersedes the 55-case run before four obsolete tests were deleted and one new regression was added |
| `cargo test -j3 -p pi-coding-agent --test agents_core_session_factories --test agents_core_end_to_end --test agents_core_drift -- --nocapture` | PASS, 19+1+5; one subprocess-only helper ignored by ordinary discovery but invoked by its parent; `/tmp/agents-core-3-factories-drift-green.log` |
| `cargo test -j3 -p pi-coding-agent --test agents_core_end_to_end -- --nocapture` | PASS, 1 after the parent AgentSession merge; `/tmp/agents-core-3-merged-e2e.log` |
| `cargo test -j3 -p pi-coding-agent --test suite_core_5 terminal_outbox_delivery -- --nocapture` | PASS, 6; `/tmp/agents-core-3-artifact-green.log`; an earlier invocation used the retired standalone target and ran no tests |
| `cargo test -j3 -p pi-coding-agent-extensions --lib tracked_read_failure_tests -- --nocapture` | PASS, 2; `/tmp/agents-core-3-wait-watchers.log` |
| Protected strict no-emit compiler, all three existing agents-core oracle fixtures, shared source-tsconfig builder, pinned compiler, source TypeScript type mapping, approved ES2024 target/lib | PASS; `/tmp/agents-core-3-oracle-typecheck-green.log`; duplicate deterministic fixture type member removed after strict RED; no source bytes or runtime oracle scenarios changed |
| `cargo check -j3 --workspace --all-targets` | PASS before each code commit and after merge; `/tmp/agents-core-3-{cleanup,mirror,attach,merged}-check.log` |
| `cargo clippy -j3 -p pi-coding-agent-extensions --all-targets`; scoped coding-agent clippy for suite_core_1/factory/drift/e2e targets | Warning-mode PASS; existing warnings remain, zero diagnostics overlap successor changed lines; `/tmp/agents-core-3-{extensions,tests}-clippy.log`, `/tmp/agents-core-3-clippy-owned-lines.log` |

`cargo fmt --check`, `git diff --check` and `python3 scripts/source-inventory.py` pass at closure; locked inventory stays 686 rows / 186,679 lines. Manual Rust/TS changed-line readability audit found no introduced issue; no numerical complexity claim. No agents/models were spawned, no PLAN.md committed, no live provider/network/push/deploy used. Independent/workspace-wide acceptance stays with main per the lane task.
