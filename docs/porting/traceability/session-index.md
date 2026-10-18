# Traceability: `packages/agent/src/harness/session/index.ts` and `jsonl.ts`

Rust: `crates/pi-agent-core/src/harness/session/mod.rs` and `jsonl/mod.rs`. Tests: `session_memory.rs`,
`session_jsonl.rs`, `session_search.rs`, `differential_session.rs`.

| TypeScript export | Rust re-export | Tests |
|---|---|---|
| `buildSessionContext` and context types | `harness::session::{build_session_context, SessionContext, …}` | context, differential |
| `JsonlSessionRepo` | `harness::session::JsonlSessionRepo` | jsonl, differential |
| `JsonlSession*` types | `harness::session::jsonl::*` | jsonl, differential |
| `InMemorySessionRepo`, `InMemorySessionStorage` | `harness::session::*` | memory, differential |
| `Session` | `harness::session::Session` | memory, jsonl, differential |
| `types.ts` exports | `harness::session::*` | codec, memory, jsonl |

Internal modules (`state`, `jsonl::codec`, `jsonl::storage`, `jsonl::repo`) are public Rust modules so tests and
traceability can address them directly. The TS package index does not export those internals.

No behavioral branches live in these barrel modules. `jsonl.ts` only re-exports the repo and types.
