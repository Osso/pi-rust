# Extension examples L

Current plan-mode audit: [whole-module trace](example-plan-mode.md). Its exact raw UI/messages/JSONL proof supersedes the historical surrogate-projection exception and copied-session-helper path below.

## Q15 common-pin game oracle imports (2026-10-23)

Both game oracles now import actual ModelRegistry.inMemory/AuthStorage rather than retired model-runtime-test-utils. No game production/assertion changes. Snake4 and invaders6 differentials GREEN (`q15b-example_{snake,space_invaders}-current-2.log`), covering full frames, saved state, timers/disposal, collision/error branches. Historical invaders-blocker diagnostic oracle imports are migrated but it has no current Rust caller; no independent proof is claimed for that unused artifact.
## Common-pin plan prompt (2026-10-22)

Native PLAN_CONTEXT now uses frozen plan-mode/index.ts ask_questions blocking/inbox instructions verbatim. Existing planning differential RED at trace10 showed the former single clarifying-questions sentence against the frozen complete instructions (`test-example_plan_mode-registry-final.log`). No expectations changed.


## Historical pre-repin read-isolation delta

This section records the older pin only, not current common-pin parity evidence. The footer prerequisite now ports the actual common-pin [readonly-session-manager.ts](readonly-session-manager.md) and imports that unchanged module directly in its oracle; it does not use the older copy below. Extension-example oracle migration is outside that footer proof.

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
TS write-isolation follow-up `528edd416`, merged at `eb4b82d76`), because pinned AND bf55d9fd6
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

## snake.ts — done

`snake_extension` / `snake_extension_with_random`: command metadata, non-TUI
error, reverse full-session save lookup (not branch lookup), nullable clear
entries, and native custom component. State is owned, including restored saves.
`Snake`: deterministic 100ms interval, paused-resume behavior, WASD/arrows,
reverse-direction rejection, queued directions, wall/self collisions, food
rejection/retry, growth/scoring/high score, restart, ESC save, Q clear, render
cache/invalidate and interval disposal. Complete ANSI frame arrays (not stripped
text) and append transcripts match unchanged TS under a virtual clock and the
same injected random sequence. Production randomness uses existing getrandom.

Tests in `example_snake`:

- `virtual_clock_food_collisions_restart_save_resume_and_frames_match`: food,
  growth, reversal, movement/wall collision, restart, clear/save, paused quit,
  resume/move/clear. Pinned first session; fixed read/append snapshots on reload.
- `restored_state_is_an_owned_copy_and_self_collision_uses_pre_tail_body`:
  save preservation on paused ESC, self/tail collision, restart preserving high
  score, original stored snapshot unchanged after live mutations.
- `ended_save_keeps_high_score_and_narrow_width_errors_match`: new game from
  ended save, high-score carryover, directions, duplicate-width cache hits,
  widths 0/1/2/3/4/5/84. Other scripts use 12/44/100. Widths below 4 reproduce
  TS RangeError (native uncaught-error payload), not an empty-width fallback.

Every timer script verifies disposal: native clock advances two extra seconds
without renders; TS asserts no remaining interval. Source quirks retained and
classified: entering the current tail counts as self-collision before tail
removal; paused resume does not invalidate cached render until tick/invalidate;
very narrow widths throw and narrow headers/footers can exceed the border.
No core API changes or dependencies.

## space-invaders.ts — done

Historical release-opt-in blocker resolved by the [core UI input slice](core-ui-input.md).
`examples/extensions/space_invaders.rs::space_invaders_extension` is now shipped
for explicit registration. `example_space_invaders` replaces the TS-only blocker
with native/pinned differential proof through real TUI input filters. The core
slice owns the complete API/behavior matrix, source discrepancies, mutation proof
and verification limits. The old TS-only oracle remains the pinned side of the
native comparison; it is no longer the sole proof.

## Proof summary

Logs and revision/scopes: `/tmp/pi-rust-ext-examples-l/ledger.md`.
Todo: 01 behavioral RED, 04 GREEN 2 tests, 05 native mutation RED.
Plan: 06 missing-module RED, 12 GREEN 3 tests, 19 native mutation RED.
Snake: 13 missing-module RED, 18 GREEN 3 tests, 20 native mutation RED.
Historical invaders proof: 21 TS blocker GREEN 1 test; 22 disabled-opt-in mutation RED.
Current native proof supersedes it: [core UI input](core-ui-input.md).
All mutations restored byte-for-byte; later formatting does not change behavior.
Only `cargo test -p pi-coding-agent --test example_<name>` and cargo fmt used;
no broad suites, clippy/check, delegation, model CLI, merge/push or live state.

Fixed-read helper copy SHA256: `31934eeda624c39b33619515d426476468ca6e99cda0da7dcd539206e3042ed1`.
