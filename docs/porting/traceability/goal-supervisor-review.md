# Goal review types and Supervisor status — 4f9e6f8a4

| Frozen surface | Native implementation / proof | Status |
| --- | --- | --- |
| goal-types union | Six decision variants only, each carries optional UTF-16 displayId. Removed old alias accepting approve/reject/advisory. Existing Goal/evidence/review interfaces retained. `goal_supervisor_review_groups_waiting_and_final_status_like_frozen` exercises every variant plus thrown Error messages. | proven |
| withSupervisorReviewStatus | One UUID for waiting and final status, final branch text, caught Error whitespace and Error empty-message string, returned displayId. Eight unmodified frozen/native cases. | proven |
| reviewGoalWithResidentSupervisor | Existing noSupervisor, project/DB/request options, six-kind validation retained. Cancellation registration/unregister still open. | partial |
| Rendering controller append | displayId persisted only when truthy; grouped root render only on append, no timer root repaint. Instructions with an ID hide duplicate transcript content and retain details. Native result callback signature widened mechanically at callers. End-to-end displayId propagation in scheduling remains open. | partial |
| Error/runtime exclusions | Primitive thrown values and invalid union shapes are typed-input gaps; entropy faults and JS callback scheduling oddities follow repository scope. | gap |

The native decision now models the source goal-only union. It does not widen the control database SupervisorResponse protocol with UI-only fields.
