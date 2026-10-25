# Goal rendering and countdown — frozen 4f9e6f8a4

Sources: `extensions/goal/src/rendering.ts`, `wait-countdown.ts`.
Native files: `bundled_extensions/goal/{rendering,wait_countdown}.rs`.

| Source surface | Native behavior and proof |
| --- | --- |
| grouped status renderer, session-qualified displayId, first-entry ownership, replay and identity-guarded cleanup | `goal_rendering_grouped_entries_match_frozen_lifecycle` compares rendered lines with the actual frozen factory |
| CountdownText captured labels, stable same-width height and width reset | grouped factory source comparison; owner repaint test renders inside the callback and checks reserved wrapped height |
| rejected scoped repaint rollback, no clock reads on a root render | `goal_countdown_rejected_scoped_repaint_preserves_visible_text` |
| bind replacement, stale cleanup token, expired rejection retry, accepted expiration stop, invalid/past start, clear-session/all | `goal_countdown_replaced_binding_retries_rejected_expiry_then_stops_like_frozen` compares actual LocalSet callbacks with the frozen refresher |
| controller append persistence, displayId grouping, hidden instruction delivery | eleven-case management differential compares persisted status text and full hidden messages; random displayId must equal the final persisted status ID before normalization |
| controller restore: empty, future, skip unrelated latest entry, latest missing/invalid/past deadline | `goal_status_restore_uses_only_the_latest_status_deadline_like_frozen` compares actual native timer repaints with the frozen controller/refresher across six cases |
| instruction wrapper/body and supervisor message rendering | existing module tests plus exact management message differential |

The Send+Sync renderer keeps components in an owner-thread-local map. Timers send
scoped repaint requests to that owner's LocalSet; they do not access TUI components
from a Tokio worker or request a root redraw. Cleanup removes only its own binding
generation. The old native-only stateless renderer had no consumers and was removed.

The dedicated restore-timer proof gap is closed. Evidence:
`/tmp/bundled3-goal-final-matrices.log` (35 goal tests),
`/tmp/bundled3-goal-restore-proof.log` (six restore cases in one test).

Pathological dates/timer overflow and Node/Tokio scheduling races are gaps under
the repository parity policy; no new carrier or API was added for them.
