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
