# Extension examples L

## Approved post-pin delta

Alessio approved “Fix TS root” on 2026-10-20. TS commit
`bf55d9fd66f55fb57c91099c14fb6b5ca2bbca51` isolates extension-facing session
entry reads. Rust already returns owned snapshots; no core change is needed.
Pinned runtime behavior is retained except for these explicitly approved read
isolation semantics.

The mandatory sandbox hides `/home/osso/Repos/pi` source files. The oracle uses
`oracle/extension-examples-l-readonly-session-manager.ts`, a byte-identical copy
of that commit's helper, wrapping the pinned runner's extension context exactly
as the fixed runner does. This proves the fixed read contract, not full runtime
parity with the newer repository. No sandbox mounts or host state changed.

## todo.ts — done

Existing native factory, tool actions (list/add/toggle/clear/errors), renderCall,
renderResult, TodoList keyboard/drawing and session_start/session_tree restore
remain covered by `todo_state_errors_branch_restore_and_all_renderers_match`
against the pinned module. No production change was necessary.

`todo_stored_details_are_isolated_on_branch_back_and_reload` replaces batch F's
ignored test. Concrete two-branch snapshots, add after restore, branch back,
reload, list and stored-entry read-back match the fixed helper. Original stored
todos and nextId remain unchanged; in-memory additions disappear after reload.
The pinned oracle fails this scenario (01); the fixed-read oracle passes (04).
Mutation of the native add-result text fails the behavioral test (05), reverted
byte-for-byte. Typed Rust input exclusions remain unchanged.

No dependencies added. Historical batch F's alias blocker is superseded here.

## plan-mode/index.ts — done

`plan_mode_extension` registers the flag, /plan, /todos, Ctrl+Alt+P, tool_call,
context, before_agent_start, turn_end, agent_end and session_start handlers.
`planning_toggle_filter_safety_execution_and_completion_match` covers metadata,
normal/planning tool selection and restoration, allowlist blocking, stale-context
filtering (string and blocks), all three choices plus cancellation, refinement
trim/follow-up messages, extraction, partial/full execution progress, completion,
status/widget ANSI and persisted snapshots. Unchanged scenarios use pinned TS.
Execution uses approved append snapshot semantics (parent approval 2026-10-20;
TS write-isolation follow-up SHA pending from main), because pinned AND bf55d9fd6
still alias caller-owned append data. Oracle snapshots actions at call-time and
clones append inputs only for fixed scenarios. This is not a claim that the
read-only helper alone fixes writes.

`restoring_execution_scans_only_after_marker_without_mutating_stored_todos`
proves flag override, execution restoration, scan marker cutoff, completed widget
and stored-entry isolation against the byte-identical fixed-read helper.

`truncated_astral_todo_projects_surrogate_at_native_append_boundary` proves a
valid astral input split by helper truncation becomes U+FFFD at append, in-memory
snapshots AND persisted JSONL. The helper stays lossless; native String/Value
cannot contain a lone surrogate. This is the approved exclusion in
`docs/specs/compatibility.md:177` (parent clarification 2026-10-20): TS persists
an escaped `\ud83d`, Rust persists U+FFFD. Oracle normalizes lone surrogates only
for this explicitly projected scenario. No other Unicode normalization occurs.

Historical batch G blocker is superseded. No core API changes or dependencies.
