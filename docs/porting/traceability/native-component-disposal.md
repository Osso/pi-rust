# Native component disposal through scoped chat

Frozen optional `Component & { dispose?: () => void }` dispatch is used by custom-entry.ts:81–88 and render-region-container.ts:89–97. Rust `Component::dispose` is the approved default no-op hook; existing native disposers must override it rather than silently inheriting that default.

## Complete existing-disposer inventory

| Native components | Trait dispatch |
|---|---|
| `StatusIndicator`, working/retry/compaction/branch-summary wrappers | matching inherent `dispose` (loader stop; retry additionally disposes countdown) |
| `ModelSelectorComponent`, `ToolExecutionComponent` | matching inherent disposer (close/refresh cleanup; timer retirement) |
| `BorderedLoader`, `ExtensionInputComponent`, `ExtensionSelectorComponent` | matching inherent disposer (loader/countdown retirement) |
| `ArminComponent`, `DaxnutsComponent`, `BashExecutionComponent` | matching inherent disposer (animation/loader retirement) |
| pi-tui `CancellableLoader` | matching inherent disposer (stop without inventing cancellation) |

Fourteen existing native component disposers wired. `CustomEntryComponent` already implements the hook directly; `ExtensionComponentAdapter` already delegates the extension UI trait's disposer, including bundled/example components. Search of all crates for `fn dispose` also finds non-components (region/fixed-cell handles, countdown controller, session/process/runner/runtime/provider objects) and already-dispatched extension UI traits; these are not pi-tui Component implementations. Footer's frozen method is empty; Rust default no-op already matches (its separate legacy free function is not an inherent component method).

## Behavioral proof

`render_region_container::scoped_chat_removal_retires_a_native_retry_timer` mounts a real native retry indicator as a tracked chat child, removes it through the container, then advances past its countdown deadline. No tick/render request or frame mutation may occur. RED `test-native-dispose-scoped-red.log` ticked after removal. GREEN1/1 `test-native-dispose-scoped-green.log` preserves request count and exact frame. Artifacts under `/tmp/pi-rust-repin-lossless-interactive/`.

This is a scoped-chat → trait-object → native-timer lifetime test, not an assertion of internal disposer calls or source shape.
