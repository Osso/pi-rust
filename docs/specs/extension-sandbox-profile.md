# Extension sandbox profile

- [x] Runner and command-context setters use the runner's session/settings, independent of command-action binding or reset.
- [x] Reject missing settings, a session override on a non-persisted session, and an inherited profile outside session scope with frozen messages.
- [x] Persist a session override before applying it to live settings; configured global/project writes use the matching settings scope.
- [x] Update the current UI's bwrap status with its own theme; restricted profiles on unsupported platforms use the frozen error status.
- [x] Reject stale contexts before mutation; propagate persistence, settings and UI errors through existing carriers.

Source: frozen `4f9e6f8a4` runner.ts:949–973. Proof: [traceability](../porting/traceability/extensions-runner.md#sandbox-setter-audit-2026-10-25).
