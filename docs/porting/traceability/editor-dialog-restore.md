# editor-dialog-restore.ts

Frozen `4f9e6f8a4`, `coding-agent/src/modes/interactive/editor-dialog-restore.ts:1–12` fully compared. Native `src/modes/interactive/editor_dialog_restore.rs`.

| Source branch | Native | Proof |
|---|---|---|
| :4–5 snapshot children and current focus | cloned ComponentRef vector and focus handle | pinned differential, original child mutation after capture remains visible |
| :7–9 clear current children then restore captured identities/order | callback clear/add loop | exact two successive restore frames, raw-surrogate content |
| :10 restore focus including undefined | native Option<ComponentRef> | differential with focused second child and absent focus |
| :11 request render | native InteractiveTuiHandle request | real nested extension-input cancellation restores prior slot and focus before resolving |

Consumer matches frozen interactive-mode.ts :2877/2952/2992 and :3129–3134: Select/Confirm/Input/Editor capture before mounting and restore before responding; custom component close/asynchronous rejection restores saved text then captured slot (non-overlay only). Overlay behavior and synchronous factory error ordering stay unchanged. This does not claim whole interactive-mode parity or new abort/disposal behavior.

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:
- RED `q13-editor-restore-red.log`: real nested input cancellation discards two prior children and reconstructs only default editor.
- Post-commit native/differential/custom-consumer proof pending.

Rc handles preserve native component identity. The helper captures mutable child objects, not serialized snapshots or default-editor fallbacks; exact JsString output survives repeated restoration.
