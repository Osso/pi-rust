# Lifecycle coordinator

Match frozen `core/lifecycle-coordinator.ts` at `4f9e6f8a4` for realistic inputs.

- Child preparation does not persist. Running/failed child commits enforce revision 1 and preserve ownership authorization, parent restrictions, and atomic lifecycle/mailbox updates.
- Attachment, steering, detach, cancellation, recovery and terminalization retain the frozen result tags, state transitions, exact error text and native text/JSON values.
- Detached cancellation forwards optional terminal-notification suppression to the transactional repository. It preserves the output label and reason, and does not require the caller to be the detached runner process.
- Successful dead-detached reconciliation clears a pending retry. If it reconciles any runtimes, it runs artifact cleanup using the supplied root (or the configured default) and the supplied timestamp.
- SQLite contention returns zero and schedules one retry per DB path after 250 ms. A retry retains the original artifact root. Successful reconciliation cancels the pending timer; an obsolete timer must not replace or consume a newer timer.
- Unexpected synchronous failures propagate; unexpected retry failures use the shared uncaught-error authority, never silent discard.

Proof map: [lifecycle-coordinator traceability](../porting/traceability/lifecycle-coordinator.md).
