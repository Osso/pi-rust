# coding-agent debug-repl traceability

Pinned source: `packages/coding-agent/src/core/debug-repl.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Status: **gap** — user decision 2026-10-18.

The module evaluates arbitrary JavaScript against a `node:vm` context (`createContext`, `runInContext`, `new Script` for the incompleteness check, `util.inspect` for rendering) and exposes `pi.agent`, `pi.runtime`, `pi.services`, `pi.session`, `pi.store`. The Rust port has no JavaScript engine, and the exposed values are live JavaScript objects (agent, session, store) that cannot cross a typed Rust boundary.

No Rust implementation, tests, or differential proof exist for this module. The socket transport, handshake validation, audit trail, and prompt state machine are not ported either, because they exist only to serve in-process JavaScript evaluation.

Consequences:

- `/debug` REPL attachment remains unavailable in the Rust port.
- The coverage row records `gap`; no other module depends on this file.
