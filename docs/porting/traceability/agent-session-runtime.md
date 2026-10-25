# agent-session-runtime.ts traceability

Rust: `crates/pi-coding-agent-core/src/core/agent_session_runtime.rs`.

## Whole-module audit closure (2026-10-25)

The locked718-line module at `4f9e6f8a4` is accepted at the user-observable bar. Audit covers every constructor/getter/setter, exported result/factory and services/Error re-export, lifecycle operation and helper. Existing pinned process/persistence proofs remain valid where behavior is unchanged; no fault/extreme matrices were repeated. Native service/Error re-exports now match the frozen module; SDK example13 and the runtime integration fixture compile through that public surface.

| Capability | Concrete proof |
|---|---|
| Constructor/apply/getters/hooks, factory DTO, initial creation and exports | actual frozen initial-event absence/new/resume differential; real convenience factory → SDK → bind_extensions event; compiled example/public imports |
| FIFO transitions, prepared permit, rebind/withSession, teardown and hook rejection order | held-gate/worker-owner UI differential; runtime callback native/raw matrices; before-invalidate retry and shutdown/empty-cleanup tests |
| Cwd resolution, relocation, beforeRun/afterTurn recovery and resource ownership | actual source recovery cases, real local provider deleting its cwd, isolated physical-cwd repair, trust forwarding and prepared activation proofs |
| New/switch/fork/reload | cancellation/selected text, metadata-before-factory, archive restoration/availability/cwd guards, old-only fork guard removal, live setup writes across await, exact UI options/reload/source frames |
| Restart/import/dispose and initial cleanup | actual default execve restart + interruption resolution/persistence; import copied/lexical/symlink/name/missing/cwd/error cases; quit shutdown order; initial detached cleanup/idle archival before factory |

Final behavior after parent integration `0a582ca63`: `runtime-event-final-runtime.log` =48/48 plus the explicitly parent-invoked isolated cwd helper; `runtime-event-final-ui.log` =4/4 bounded startup/import/switch-fork/rebind tests. Workspace all-targets passes0 errors/0 warnings for API commit `e4a325a8d`, then the requested -j3 integration gate also passes0/0 (`train-de46-workspace.*`). Strict common-pin TS and Biome pass. Scoped core/interactive clippy156 existing diagnostics, zero owned changed-line primary spans. Manual changed-line Rust/TS readability audit: typed events remove string conversion/defaulting; no new carrier, fallback or suppressed warning. The final public re-export/import cleanup also passes the required `cargo check -j3 --workspace --all-targets` with0 errors/0 warnings (`runtime-public-workspace.*`); only visibility/imports changed after the behavior proofs. Locked inventory passes686/186679 with610 done /25 gap /28 partial /23 todo. Logs: `/tmp/repin-interactive-2-proof/`.

Frozen Agent's `initialState?.model ?? DEFAULT_MODEL` keeps the ordinary no-auth model sentinel defined, so the runtime's model-persistence branch matches without a new optional-model API. Rust getters return approved owned values; built-in hosts retain runtime ownership while using bound actions. Pathological JSON, OS-fault shapes, numeric extremes and runtime scheduling remain the single recorded project gap, not new APIs or matrices. UI-specific reset/render/resource acceptance belongs to the still-partial [interactive mode](interactive-mode.md), not this runtime module. Historical open checkpoint statements below are superseded by this closure.

## Initial session-start event API (2026-10-25)

Main-approved correction: `CreateAgentSessionRuntimeOptions` and initial creation use the existing `Option<SessionStartEvent>`. The flattened string reason/previous fields and main's lossy reason conversion are deleted. Lifecycle replacements construct their typed event; initial creation passes its optional event unchanged. Main, SDK example13, test RPC-demo harness and the Rust convenience factory forward it to the existing SDK option. Fixture observers and all creation callers are mechanical fallout, including the resource-display-home test helper (the276 production resource block is untouched).

Genuine actual-source/runtime RED `runtime-initial-event-red.log`: source factory receives no initial event; native fabricated startup. `initial_runtime_preserves_absent_and_explicit_session_start_events_like_frozen` compares absence, new, and resume with a lone-unit previous path plus the unchanged factory rejection. `convenience_factory_emits_the_requested_session_start_event` creates the real SDK-backed runtime, binds extensions at the actual emission boundary, and observes the requested resume payload. No new carrier, alternate path, dependency or source delta.

## Missing-import error identity (2026-10-25)

Actual frozen `importFromJsonl` returns `SessionImportFileNotFoundError` with `name`, `message` and `filePath`; native previously projected only its message into a generic Error. `missing_import_preserves_the_frozen_error_name_and_file_path` reproduces the complete visible error-value mismatch. The existing `SessionImportFileNotFoundError` now projects its fields into `NativeNodeError`/`CallbackError` and retains itself as the original Rust source; no new error carrier or constructor class is added.

The interactive catch now checks that retained source instead of `File not found:` text. `import_error_class_controls_recovery_and_exit_like_frozen` uses real native/frozen runtime/mode processes, a persisted private session and an actual imported JSONL file. A genuine missing import remains recoverable with the exact full error frame and process exit0. A later factory Error with the same prefix must produce the same full frame and actual exit1; native previously exited0. The frozen confirmation dialog is driven by real terminal Enter input, never replaced with a canned result. Both process scenarios pass after the fix. RED/GREEN artifacts: `/tmp/repin-interactive-2-proof/import-error-red.log`, `import-ui-persisted-red.log`, `import-{agent_session_runtime,lib}-green.log`. Separate whole import-command parsing/confirmation/recovery branches remain interactive work, not accepted by this bounded error proof.

## In-memory import discrepancy (2026-10-25)

DISCREPANCY, preserved literally: frozen :637–640 recursively creates the current manager's session directory before before-switch callbacks. An in-memory manager supplies `""`, and Node throws ENOENT. Rust `create_dir_all("")` instead succeeds as a no-op. Native now performs the actual empty-path mkdir syscall for that input and preserves the resulting fields through the existing Node filesystem error carrier. Nonempty paths still use recursive creation. The actual frozen/native UI process comparison proves the exact visible failure and exit1; no new recovery, fallback, carrier or SessionManager algorithm is introduced. [Command proof](interactive-mode.md#path-arguments-and-import-command-2026-10-25).

## Common-pin re-port in progress (2026-10-23)

[Contract](../../specs/agent-session-runtime.md). Historical inventory below is
not whole-module acceptance. Parent cleared the cloneable handle migration after
confirming worker-thread relocation consumers: agent-core `agent_loop.rs:43/64`
uses `tokio::spawn`; `agent_session.rs` activates the prepared relocation from
AgentEnd; runtime relocation handlers are Send futures. Send carriers remain.

| Frozen boundary | Native checkpoint | Proof |
|---|---|---|
| :167–185 acquire/runLifecycleTransition FIFO | one private Tokio owned mutex permit; no outer runtime operation lock | actual frozen prepare/new/activate vs real native concurrent lifecycle test |
| :369–408 prepare retains permit through activation and releases before continue | owned permit stored with prepared relocation; released on activation success/error | `prepared_relocation_holds_lifecycle_gate_until_activation` RED `runtime-lifecycle-red.log` (new completed early), GREEN `runtime-lifecycle-green.log` |
| constructor/apply bind runtime actions | cloneable Arc-inner handle, short state mutex, weak handlers; obsolete share wrapper deleted | same real-runtime gate test compiles constructor/weak handlers |
| :418–420 reload under lifecycle gate | runtime forwarding under same permit; interactive /reload and extension reload now use it | suspended-loader success/raw-failure source/native UI differential; [bounded body proof](interactive-mode.md#reload-body-checkpoint-2026-10-23); serialization shares held-rebind proof |
| beforeInvalidate/beforeRestart/rebind ordering (:220–229/:277–285; interactive :666–672/:2142–2203) | optional host scheduler; constructor installs UI-owner hooks, synchronous reset before dispose; rebind awaited under sole permit; alternate adopt_session and UI session RefCell deleted | worker runner prepare→activate RED/GREEN plus queued-rebind characterization below; individual reset/restart side-effect branch proof still pending |
| whole source branch coverage | 4b working-directory recovery bounded proof below; archived resume/trust propagation, restart cleanup, teardown and creation side effects still auditing | not whole-module acceptance |

## Working-directory support prerequisite (2026-10-23)

[Session cwd whole-module matrix](session-cwd.md) replaces infallible fallback discovery with native-error Result propagation. Runtime creation/switch/import pass that result unchanged; interactive resume uses the caught typed error rather than rereading JSONL. [Shared process cwd/chdir](process-cwd.md) uses direct native process state and preserves restart cwd errors; Node's cached previous-path/notice behavior is an approved gap. These support checkpoints alone do not accept the before-run/after-turn working-directory recovery hooks.

## 4b recovery checkpoint (2026-10-23)

Frozen runtime :233–265 now binds optional AgentSession recovery callbacks through weak runtime handlers. Before run finds the nearest existing directory, repairs native process cwd, and relocates if session cwd differs. After turn skips already-prepared relocation, repairs native cwd and prepares through the real ExtensionRunner method. The free :64–76 repair helper preserves expected missing-path catches and native failures; the approved cached-path notice gap remains explicit in [process cwd](process-cwd.md).

`continue_recovers_deleted_session_cwd_before_empty_transcript_returns` compares actual frozen Runtime/AgentSession prototype methods (only factory/agent collaborators supplied) against the real native runtime after removing an owned child project directory. Genuine RED: source moves to parent, native remained at deleted child (`runtime-cwd-before-continue-red.log`). Initial continue boundary is GREEN at145bfa8d (`runtime-cwd-before-continue-green.log`). Current proof is composite26 nonignored tests: full original+callback target22/22 at5e32882a, followed by four independent focused tests added without production changes; the ignored physical child is invoked explicitly by its parent. No redundant full-target rerun.

| Frozen capability | Native proof / boundary |
|---|---|
| Idle continue, prompt and registered-command beforeRun; recovery before unknown-command validation | actual source prototypes vs real runtime after directory deletion; actual command callback observes replacement context.cwd; real local provider completes the forwarded prompt |
| Streaming skips beforeRun while commands remain callable | in-flight per-harness provider held by Notify, continue rejects already-processing and registered callback observes live cwd; actual frozen methods agree (`runtime-cwd-streaming-guard.log`) |
| Replacement identity and lock lifetime | callback resets its own handlers, reenters continue/start lock, then returns the same session; bounded timeout succeeds, no state mutex/old permit held through callback |
| Raw/native/DOMException callback sources | raw lone-surrogate object beforeRun rejection stays outside the command catch; Native/DOMException branded properties round-trip through boxed AgentSessionError source |
| afterTurn precedes unchanged stop predicates; existing prepared relocation wins | two actual source Runtime+AgentSession+ExtensionRunner scenarios vs real native lifecycle; prepared target and stop=true agree, existing owned permit does not deadlock (`runtime-cwd-after-turn-preparation.log`) |
| Real response → afterTurn prepare → AgentEnd activation | local provider removes session project during its successful reply; real runtime moves to source-selected surviving parent, consumes preparation (`runtime-cwd-real-turn.log`) |
| afterTurn error boundary | actual frozen Agent.runWithLifecycle catches the installed hook's RangeError and emits error AssistantMessage; native real prompt matches stopReason/message including lone surrogate. Public prompt resolves by source design, not a dropped exception |
| Deleted physical process cwd and visible notice | isolated native child deletes actual cwd; actual source repair/continue produces the same repaired cwd and customType/content/display payload (`runtime-cwd-physical-notice.log`). Cached previous-path text remains the approved runtime gap |

`runtime-cwd-hook-boundaries-final-green.log` covers22/22 including original lifecycle cases. Direct provider injection fixes the fixture's parallel global-registry race; no test serialization, production workaround or source expectation replacement. Failed attempts and the isolated diagnostic remain in the ledger.

Integration at ff1d34e3: workspace all-targets0errors/0warnings (`runtime-cwd-final-workspace.{jsonl,log}`). Diagnostic-only targeted clippy --lib/--test exits0 with545 existing warnings and **zero owned-hunk findings**; not a strict full-clippy claim (`runtime-cwd-clippy-owned.json`). Pinned TS/Biome and final fmt proofs are in the ledger. Native production files were unchanged after these checks; subsequent additions are independent tests/oracle cases.

Manual Rust/TS changed-line readability audit; metrics binary absent. Three caller conditions share one short recovery helper; callback/state guards end before awaits. New runtime helpers are bounded and effect-named. Source oracle loops supply only agent/factory collaborators and invoke real pinned recovery methods. Existing long prompt/command orchestration and the pre-existing large integration fixture are not redesigned. Whole runtime acceptance still requires the remaining4c/4d branches.

## 4c.a empty cleanup checkpoint (2026-10-23)

Frozen :228 now calls [empty-session cleanup](empty-session-cleanup.md) after beforeInvalidate and before dispose. Native's duplicate dispose-only teardown and settle-active-response alternate path are deleted. All transitions use the frozen shutdown→beforeInvalidate→empty cleanup→dispose order with one captured session; no pre-abort or silently ignored abort error. Genuine end-to-end RED→GREEN removes both header-only file and metadata before disposal. Source/native EISDIR failure preserves metadata-first side effect and keeps outgoing context live; full current Runtime regression28/28 passes. The [module matrix](empty-session-cleanup.md) also covers20 guard/sweep scenarios and actual CLI startup/standalone policy. This accepts that bounded cleanup call, not later idle archive or whole runtime.

## 4c.b idle archive checkpoint (2026-10-23)

Frozen createAgentSessionRuntime :699 starts [idle archival](idle-session-archive.md) after detached cleanup and before awaiting the runtime factory, guarded by control DB truthiness and standalone mode. Native now calls the fire-and-forget wrapper reexported from existing session_archive_storage. Real frozen startup RED: a rejecting factory still leaves an old transcript archived/indexed in source, while native never archived (`idle-archive-startup-red-2.log`). [Idle archival checkpoint](idle-session-archive.md) records the completed bounded source/native matrix, process discrepancy, native publication boundaries and workspace0/0; shared dependency carrier remains deferred.

## 4d restart/new work (2026-10-23)

- Frozen runtime:438 detaches all registered tools before either restart path. Source registry+AgentSession wrapper+runtime prototype and actual native runtime match insertion order with failed/successful detach handles (`restart-detach-all-{red,runtime-green,registry-green}.log`, implementationd6f01524).
- Frozen runtime:426-433 persists active model and thinking before beforeProcessRestart. Genuine `runtime-persist-red.log`: source hook observes faux/faux-1/off; native observed null/null/null. Native helper now saves both before persistForRecovery; GREEN at a99021d3 (`runtime-persist-green.log`).
- Frozen runtime:509-517 sets metadataControlDbPath on a new SessionManager before factory. Genuine `runtime-new-db-red-2.log`: source factory inherits DB, native lacked it. Native now copies the outgoing DB before optional parent setup and factory; GREEN at a99021d3 (`runtime-new-db-green.log`). Initial source fixture omitted services.cwd and was corrected before RED classification.
- [Self-restart module](self-restart.md) records notice/Args/resolver proofs and frozen lost-DB-on-persisted-restart discrepancy. No DB transfer added to restart; new-session transfer is a separate explicit frozen branch.

- Frozen switchSession:313-325 reads truthy outgoing manager metadata DB, restores archive before open, binds DB on the new manager, then validates cwd/availability before teardown/factory. Actual source switch prototype vs native runtime REDs: archived open prevented the native factory (`runtime-switch-archive-red.log`); missing-manager-DB returned the wrong archive-open error (`runtime-switch-missing-db-red.log`). Native now preserves exact ordering and manager authority rather than session.config DB. Source/native assert restored plain/archive presence and DB binding at factory boundary; GREEN at b5fe75de (`runtime-switch-{archive,missing-db}-green.log`).

- Frozen activateRelocation:359 repairs physical process cwd before teardown. Real isolated child/source prepareToolResultRelocation→delete physical cwd→activate differential RED (`runtime-relocation-process-repair-red-2.log`): native beforeInvalidate sees missing cwd; source sees target. Native now calls the shared repair helper before teardown. Initial probe deleted cwd before preparation and hit the approved Node cached-cwd gap (`runtime-relocation-process-repair-red.log`); corrected fixture reproduces the actual prepare/activate failure boundary. GREEN at91595010 (`restart-relocation-repair-green.log`).

- Frozen switch:327 and relocation:373 forward optional ProjectTrustContext to factory after teardown; switch invokes the optional context factory only then. Required Rust carriers/signatures replace old switch arguments without wrappers. Behavioral REDs `runtime-trust-forwarding-red.log` and `runtime-relocate-trust-forwarding-red.log` prove lost supplied context, omitted context callback, and clone-only relocation hiding the changed cwd from outgoing shutdown consumers. Native now forwards the option and mutates the actual outgoing SessionManager before retaining its owned replacement snapshot, as frozen built-in consumers require. GREEN atb25fab9d (`runtime-trust-forwarding-green.log`). Main factory source :1156-1163 prefers a supplied context over its default context; real native factory→loader→project_trust extension-handler RED (`main-trust-context-red.log`) observes print/false instead of supplied rpc/true. Native now forwards the provided context into the actual trust resolver. This nested TS main factory is not independently loadable; main proof is native behavioral plus literal source branch audit, not a claimed whole-main differential. Main consumer GREEN (`main-trust-context-green.log`). Interactive source createProjectTrustContext:2786-2798, /cd:6285-6286 and resume:6673-6693 now supply TUI/hasUI context through the live native UI bridge. Actual source UI prototype methods vs capture-terminal native command/factory REDs (`interactive-trust-context-red.log`, `interactive-resume-trust-context-red.log`) showed null context for /cd and resume. Native now resolves /cd before context creation and passes optional context factory for resume/current-cwd recovery and extension requests; GREEN atbaef4cdf, including a relative /cd fixture (`interactive-trust-context-green.log`). Print/rpc paths supply no context as frozen; call-site changes only adopt the replacement options carrier.

### Bounded4d checkpoint

Revision98953083 includes parent8b15a5c7. Current `cargo test -j4 -p pi-coding-agent --test agent_session_runtime -- --nocapture` passes38/38 with one isolated child entry marked ignored and explicitly invoked by its passing parent. `self_restart`21/21 and `self_restart_state`4/4 pass; actual CLI handoff and main trust factory focused tests pass; focused UI context and syscall classification lib tests pass. Exact outputs: `/tmp/pi-rust-repin-lossless-interactive/4d-final-{runtime,self-restart,state,main-trust,cli,ui,kill}.log`.

Requested workspace JSON check exits0 with0 errors/0 warnings (`4d-final-workspace.{jsonl,log}`). Scoped lib + six target clippy exits0 with0 changed-hunk findings relative to parent8b15a5c7;537 existing other findings remain explicit (`4d-final-clippy{.jsonl,.log,-owned.json}`). Strict pinned-source-config tsgo + Biome for all three new oracles pass (`4d-{typecheck,biome}.log`). Format before every commit. Manual changed-line Rust/TS readability audit; metrics binary absent. Source-shaped typed result construction in self_restart remains longer than the30-line heuristic, without branching/identity/error suppression abstractions added solely to shorten it.

Remaining4d work is **not** an approved gap: fallible beforeProcessRestart/setup/withSession callback carriers and extension_actions adapter ordering; interactive RuntimeSwitchSession must preserve withSession options. Main granted semantic ownership for those follow-ups. Broader self-restart dependency/error-carrier audit remains explicit in [self-restart](self-restart.md). No whole-runtime acceptance; rank5/6 remain queued. Later edits for this checkpoint are docs only and do not invalidate these behavior/check proofs.

## Setup shared-manager correction (2026-10-25)

Frozen new-session :517–521 passes the live manager and awaits setup before rebuilding messages. Both native setup aliases now take the existing `SharedSessionManager`; extension_actions forwards that same handle. The manager clone/publish path is deleted. Callbacks take short locks only while reading/writing, never across await. Main approved this callback parameter correction; mechanical fallout is limited to the extension alias/adapter and callback test consumers in this checkout.

Actual pinned/runtime extension-action RED `setup-async-red.log` proves the old snapshot publication erased a synchronous live-manager entry: source retains two concrete entries and syncs two messages; native retained only the later entry/message. `setup_writes_across_await_keep_live_state_and_sync_only_on_success_like_frozen` compares success and rejection after a real yield against frozen await behavior: ordered payloads remain live in both cases, success syncs two agent messages then rebind/withSession, rejection keeps zero agent messages and skips those steps. The existing four synchronous setup/replacement error cases still assert exact native/raw rejection identity. No new carrier, recovery path or dependency is added.

## 4d fallible callbacks checkpoint

Frozen beforeProcessRestart/setup/withSession awaits can reject; native now carries Result<_,CallbackError> through all three optional callback types. The non-interactive CallbackErrors recovery box is deleted: adapters convert the original error at the actual await, so failure stops the runtime before subsequent stages. Interactive new-session adapters reuse the same direct conversions instead of discarding future Results. Setup writes affect the live manager before and after awaiting its future (see the shared-manager correction above); rejection retains those entries without syncing agent messages, rebinding or calling withSession. No manager mutex is held across the await.

Genuine real-source/runtime/extension-action RED `runtime-callback-setup-red.log`: source calls teardown/create/setup, retains the new entry and leaves agent messages empty; native additionally synced messages and ran rebind/withSession. New matrix asserts exact native RangeError branding/properties and raw rejection objects with lone high/low UTF-16 units. A source/native before-restart rejection test checks detach→hook, live outgoing context, and no teardown/restarter. GREEN at410ebd3d (`runtime-callback-carriers-green.log`): setup/withSession native+raw and beforeRestart native+raw cases. UI lost options are separately reproduced below, not inferred from carrier proof.

Actual source bindCurrentSessionExtensions actions + Runtime prototypes/SessionManagers vs native live command context→UI channel→runtime REDs: switch callback was dropped (`interactive-switch-options-red-2.log`); fork defaulted At, dropped callback and left editor unchanged (`interactive-fork-options-red.log`). New actor payload preserves switch options and uses the shared resume handler; fork forwards withSession, defaults Before, clears pending summary edit, applies selectedText (including lone surrogate) and shows frozen success status. GREEN at7e35d3e0 (`interactive-session-options-green.log`): switch, default-Before fork and explicit-At fork. No prior assertions removed.

Frozen new:515 and restart:462 use truthy parentSession/notice guards. Actual source/runtime REDs (`runtime-parent-truthiness-red.log`, `runtime-notice-truthiness-red.log`) show native incorrectly storing empty parent/notice strings. Native now skips only empty values, retaining nonempty UTF-16 notices exactly; three-case positive/absent/empty matrices include lone high/low units and NUL. GREEN ate4736aef (`runtime-{parent,notice}-truthiness-green.log`).

Current callback integration:42 runtime tests GREEN plus the separately invoked isolated child; new UI option/trust and caught-cwd regression probes GREEN. Existing selector probe initially exited at the now-correct manager-DB guard (`4d-callback-final-selector.log`): its fixture replaced the outgoing manager without restoring metadata DB. Fixture now preserves that production binding; every original assertion remains unchanged. Focused selector recheck GREEN at29b06c33 (`4d-callback-selector-green.log`). This is the same guard independently proved against frozen in `runtime-switch-missing-db-{red,green}.log`, not a production fallback.

Checkpoint29b06c33 includes parent9d889e22 via65b6b508. Runtime42/42 plus explicit isolated child passes (`4d-callback-final-runtime.log`); focused option/trust/caught-cwd/selector UI probes pass (`4d-callback-final-{ui-options,ui-trust,missing-cwd}.log`, `4d-callback-selector-green.log`). Requested workspace all-targets JSON check0 errors/0 warnings after final fixture edit (`4d-callback-selector-workspace.{jsonl,log}`). Scoped clippy lib/runtime target0 changed-hunk findings,535 existing other warnings (`4d-callback-final-clippy{.jsonl,.log,-owned.json}`); final selector edit is cfg(test)-only and outside that command's lint scope. Strict tsgo/Biome new oracles pass (`4d-callback-{typecheck,biome}.log`). Source inventory PASS:686 rows/186679 lines, statuses399/19/88/180 (`4d-callback-selector-inventory.log`). Manual Rust/TS readability audit; no metrics binary installed. No source/API acceptance is inferred for the still-open self-restart injected-process dependencies.

**Queue7 ported2026-10-24:** live result/timestamp cache, summary-edit consumer and fork/reset/navigation clears now have a native counterpart. Source/native live frames, repeated edits, timestamp retention, navigation success/cancel/occupied editor, reset and fork-option probes are recorded in [interactive-mode live display](interactive-mode.md#live-compaction-display-2026-10-24-queue7). This supersedes the historical ignored-result/persisted-only rebuild gap; no whole-mode acceptance.

## 4c transition queue-event correction (2026-10-23)

Frozen :220–230 does not call abort. A real pinned AgentSession+AgentSessionRuntime probe subscribes to queue_update and throws from beforeInvalidate. Source reports no queue event and retains the same session; native's pre-abort emitted one extra queue_update before the hook (`runtime-pre-abort-queue-red.log`). Native now deletes the pre-abort, ignored result and unused settle flag; all transitions/disposal use one source-ordered teardown. Implementation committed before focused GREEN and229's independent SDK13 probe.

## Fallible synchronous before-invalidate hook (2026-10-23)

Frozen :220–229 has no catch around beforeSessionInvalidate: a throw at :226 skips removeAbandonedEmptySession/dispose. Native hook now returns Result and teardown propagates it before dispose, under the same permit. `before_invalidate_error_skips_dispose_and_releases_gate_for_next_transition` compares actual frozen teardown/runLifecycleTransition against real native runtime: raw hook error, unchanged session ID, outgoing context still live after error, then a succeeding new transition invalidates it. Main-approved **compile-RED** against old Fn()->() API (`runtime-invalidate-error-compile-red.log`), GREEN `runtime-invalidate-error-green.log`. No panic/uncaught carrier; no state lock spans the hook. Header reset failures now use this Result path. Unrelated teardown abort/cleanup gaps remain open.

## UI owner dispatch checkpoint (2026-10-23)

`None` host executor runs inline (print/RPC/non-interactive); interactive scheduling uses only the existing UI request drain and `spawn_local`. The owning thread drains older enqueued worker submissions before spawning its own; this also lets shutdown complete after the main UI loop exits. Captured ThreadId prevents a different thread's TLS mode ID from selecting the wrong owner. Runtime UI request handlers spawn instead of blocking that drain. No state mutex guard crosses an await or callback; the runtime's private permit remains the only transition authority. `dispose` now returns CallbackError on host-delivery failure; print/RPC/interactive awaited-without-catch disposal reaches shared `throw_callback_uncaught` at their process boundaries, matching frozen print-mode.ts:61, rpc-mode.ts:735 shutdown and interactive-mode.ts:5147/5163. Example/test callers explicitly assert success.

`ui_tests::worker_relocation_rebinds_interactive_session_on_ui_owner` drives the actual Send extension runner prepare→activate path from a separate OS thread/runtime, then asserts live cwd and replacement UI extension binding before activation resolves. RED `runtime-ui-worker-red.log`: outgoing cwd remained mounted; GREEN `runtime-ui-worker-green.log`: replacement bound successfully.

`ui_tests::runtime_rebind_holds_gate_and_preserves_worker_owner_submission_order` compares frozen :167–185/:277–285/:411–415 through `oracle/agent-session-runtime-rebind-oracle.mts` with real native runtime and capture-terminal mode. First rebind is suspended; a worker submission is queued before a UI-owner submission. Neither enters before release; hooks finish first→worker→owner and run on the UI thread. **Characterization, no RED by design**, approved by main after missing-hook RED above. Log `runtime-ui-rebind-order.log`. This proves gate/scheduler order, not all rebind UI side effects.

No state mutex guard crosses an await. Callers clone the runtime handle directly;
print/RPC/interactive/main and harnesses no longer own an outer operation mutex.
This checkpoint preserves existing standalone gates, fork-guard removal and
CallbackError carriers. Logs: `/tmp/pi-rust-repin-lossless-interactive/`.


| TS | Rust | Tests |
| --- | --- | --- |
| `AgentSessionRuntime` | `AgentSessionRuntime` | `tests/agent_session_runtime.rs` (48 current nonignored tests) |
| `createAgentSessionRuntime` | `create_agent_session_runtime` | same |
| `CreateAgentSessionRuntimeFactory` | `CreateAgentSessionRuntimeFactory` | same |
| `switchSession` | `switch_session` | cancellation test |
| `newSession` | `new_session` | before-switch/shutdown/replaced-context test |
| `fork` | `fork` | invalid entry + before-fork cancellation |
| `relocate` | `relocate` | cwd + `cwd_changed` note |
| `restart` | `restart` | process path injected; default restarter uses `core::self_restart` |
| `importFromJsonl` | `import_from_jsonl` | `import_from_jsonl_switches_to_the_imported_session`, `import_through_a_symlinked_session_dir_preserves_the_source_file` |
| Session replacement re-binds the extension runner's `sessionManager` | `create_runtime` builds a new `AgentSession`, whose `build_runtime` passes the new shared handle | `session_replacement_rebinds_the_shared_extension_handle` |
| `dispose` | `dispose` | `tests/agent_session_runtime.rs::dispose_emits_session_shutdown_quit_before_invalidating_the_session` |
| `prepare/activateToolResultRelocation` | same | `bound_runtime_actions_relocate_through_the_extension_context` |
| `bindSessionRuntimeActions` | `AgentSessionRuntime::bind_session_runtime_actions` in constructor/apply | `bound_runtime_actions_relocate_through_the_extension_context` |
| `emitBeforeSwitch` / `emitBeforeFork` | `emit_before_switch` / `emit_before_fork` | cancellation test |
| `teardownCurrent` | `teardown_current` | shutdown event test |
| `finishSessionReplacement` | `finish_session_replacement` | `withSession` receives `ReplacedSessionContext` |
| `runDetachedJobArtifactCleanup` | `run_detached_job_artifact_cleanup` | invoked by `create_agent_session_runtime` |

## Port fixes

- Common-pin fork correction2026-10-23: remove the old-only file-exists/unsaved-session guard. Frozen runtime.ts:568–583 opens the manager then calls createBranchedSession, so a fork containing only restriction context followed by clone reports `Entry <leafId> not found`. Full RPC differential exercises this persisted fork→clone boundary and asserts both complete serialized errors using each side's captured leaf ID and the identical literal error template; no error output is normalized or removed.

- Standalone runtime creation/resume/import gates and proof: [standalone slice](standalone-worker.md).

- `fork` captures the current session file before locking the shared
  `SessionManager`; the earlier Rust body called `AgentSession::session_file()`
  while holding the manager guard for `newSession`, which deadlocked on the
  in-memory fork path. The TypeScript source reads `sessionFile` through the
  manager getter, so this is a port bug, not a source discrepancy. Covered by
  `ui_tests::fork_selector_forks_from_the_selected_user_message`.

## Behavior

- `dispose` emits `session_shutdown` with reason `quit` before
  `beforeSessionInvalidate` and `session.dispose()`, matching the source.
- `importFromJsonl` compares `path.resolve(destinationPath)` with the resolved
  input path lexically (`resolve_path` with tilde expansion disabled) instead of
  canonicalizing; resolution errors propagate. The source uses
  `resolve(destinationPath) !== resolvedPath`. When the lexical paths differ but
  both names resolve to the same file, the copy is a no-op, matching
  `copyFileSync`; `std::fs::copy` alone would truncate the source.
- Constructor binds runtime-owned extension actions through the one Arc-inner
  handle. `apply` re-binds them on the replacement session. The obsolete outer
  operation mutex and `share_agent_session_runtime` wrapper are deleted.
  Activation propagates relocation failures before continuing.
- Callers: `main` startup, print/RPC modes, and the interactive mode all take the
  shared runtime handle, so extension `ctx.relocate`, `ctx.relocateAfterToolResult`,
  and the tool-result activation path reach the runtime.

## Gaps

The historical project-trust/availability and pre-abort notes are superseded by the 4d and teardown proofs above. Whole-runtime acceptance is recorded in the closure above. Pathological input, OS-fault, numeric-extreme and runtime scheduling cases follow the project user-observable parity exclusions; existing lossless carriers remain intact.

## v15 import/fork names (approved 2026-10-20)

Name paths follow `c248b0067` ([reference lock](../source-lock.json)).
Copied imports remove stale destination metadata after copying; lexically identical imports
return before copy/removal. Forks reset name state rather than inheriting a parent name.
Proof: `agent_session_runtime::import_does_not_inherit_stale_metadata_or_historical_jsonl_names`,
`fork_does_not_inherit_the_parent_session_name`, existing lexical/symlink import test.
