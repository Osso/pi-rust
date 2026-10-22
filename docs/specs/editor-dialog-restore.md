# Editor dialog restoration

Match frozen `4f9e6f8a4` `editor-dialog-restore.ts`; [traceability](../porting/traceability/editor-dialog-restore.md) maps the complete helper and consumers.

## What it must do

- [ ] Capture editor-container children and focus; restore those exact references/order and focus, including absent focus, then request rendering.
- [ ] Repeated restoration retains changes to original child objects, including unmatched UTF-16 content.
- [ ] Extension dialogs restore the pre-dialog slot before resolving; non-overlay custom components restore saved editor text and slot on close/asynchronous failure.

## How it works

- [Source mapping](../porting/traceability/editor-dialog-restore.md)

## Implementation inventory

- `src/modes/interactive/editor_dialog_restore.rs`: captured restoration callback.
- `src/modes/interactive/interactive_mode.rs`: dialog/custom-component lifecycle consumers.

## Tests asserting this spec

- `interactive_mode::ui_tests::editor_dialog_restore_matches_frozen_snapshots_and_raw_children`: actual pinned helper differential.
- `interactive_mode::ui_tests::extension_dialog_restores_previous_editor_children_and_focus`: actual nested input lifecycle.
- Existing extension select/custom component tests cover normal restoration, overlay and factory-error ordering.

## Known gaps (current cycle)

- [ ] Post-commit focused execution pending.

## Out of scope

Whole dialog abort/disposal and interactive-mode re-pin remain separate Q13 scope.
