# Goal rendering and countdown — frozen 4f9e6f8a4

Source: `extensions/goal/src/rendering.ts`, `wait-countdown.ts`.

| Surface | Native behavior and proof | Status |
| --- | --- | --- |
| Status entry renderer / grouped display map | First entry owns region; later displayId entries update the captured component and return no row; session-qualified keys, replay and identity-guarded cleanup. `suite_extensions goal_rendering_grouped_entries_match_frozen_lifecycle` compares all rendered lines with the unmodified frozen module. | proven |
| CountdownText / component update | Captured labels, same-width maximum height, width reset, rejected scoped redraw rollback. Root renders no longer read the clock. `goal_countdown_rejected_scoped_repaint_preserves_visible_text`; real owner-thread callback test renders inside repaint to expose RefCell reentrancy. | focused proof |
| bind / start / clearSession / clearAll | Shared timer owns scheduling, owner LocalSet receives scoped requests; token-guarded stale cleanup; expiration retries only rejected repaint. No root UI repaint on ticks. `goal_countdown_owner_repaint_renders_due_without_shrinking_then_cleans_up`. | broader branch proof pending |
| Controller append / restore / instruction delivery | Existing native exports remain; displayId persistence, grouped requestRender and hidden grouped instructions still pending. | open |
| Runtime fault/numeric extremes | Pathological dates, timer overflow and JS scheduling races are recorded gaps under the repository parity bar. | gap |

The Send+Sync native renderer retains components in an owner-thread-local map. Entry cleanup removes only the component generation it created. Timers send repaint requests to that owner's LocalSet; they never call TUI components from a Tokio worker.
