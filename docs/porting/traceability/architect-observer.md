# architect/observer.ts

Frozen `4f9e6f8a4`; Rust `src/architect/observer.rs`.

| Export / group | Proof |
|---|---|
| session/request/observation/snapshot/state/reader types, `ARCHITECT_SESSION_ID` | Serde/native aliases and compiled consumers; [legacy branch map](architect.md#observerts) |
| `snapshotArchitectSessions`, `createArchitectObservation` | `architect` changed/unchanged/request-priority cases; frozen request-cycle differential |
| `readArchitectSnapshot` | `architect` live/newest-row/read-only/missing-DB cases; frozen request-cycle query |
| `ArchitectObserver` observe/state/claim/renew/complete | `differential_architect_request_loop`, `architect` fixture tests |

Exhaustive SQL/filter/sort differential remains unproven; prior done label is not new full-parity evidence. [Slice proof](../q16-supervisor-runtime-proof.md).
