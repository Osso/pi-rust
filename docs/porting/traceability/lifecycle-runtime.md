# Lifecycle runtime — frozen 4f9e6f8a4

Source `extensions/agents-core/src/lifecycle-runtime.ts`; native `agents_core/lifecycle_runtime.rs`.

| Export / branch | Native / proof |
| --- | --- |
| `RUNTIME_PROCESS_IDENTITY`: process-wide identity snapshot | `runtime_process_identity`; reused by real spawn, detached cancellation and ownership validation tests; core runtime-process identity has its own bidirectional proof |
| `createLifecycleCoordinator`: no persistence → undefined | `create_lifecycle_coordinator` returns None via the same persistence-target guard; memory cancellation differential proves no coordinator/ownership is fabricated |
| Persisted factory: controlDbPath/sessionPath, store allocator, current ISO clock, process snapshot | Same native coordinator options; real spawn→SQLite→TS-reader tests use this factory, including detached child cleanup and descendant suppression. Store allocator remains the single authority |
| Native `publish_coordinator_snapshot` | Runtime.ts helper lives here natively; it is not an extra frozen lifecycle-runtime export or alternate coordinator authority |

Current lane proof includes all 26 agents-core runtime tests (`/tmp/agents-core-settlement-closure-green.log`) plus the real coordinator/mailbox tree cancellation test. No semantic deviation. Gap: entropy/clock/runtime fault shapes follow the project boundary.
