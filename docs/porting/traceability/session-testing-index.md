# Session testing index

Source: `packages/agent/src/harness/session/testing/index.ts` at [source-lock](../source-lock.json).
Rust: `crates/pi-agent-core/src/harness/session/testing/mod.rs`, exposed by `harness/session/mod.rs`.

| Source export | Public native export | Proof |
|---|---|---|
| `createSessionBackendConformance` | `pi_agent_core::harness::session::testing::create_session_backend_conformance` | Downstream core and SQLite consumers invoke factory and cases |
| `SessionBackendConformanceCase` | `SessionBackendConformanceCase<F>` | Returned named cases run externally; complete pinned TS manifest compared |
| `SessionBackendFixture` | `SessionBackendFixture` | Implemented externally by both integration consumers |
| `SessionBackendFixtureFactory` | `SessionBackendFixtureFactory` | Async closure blanket implementation consumed externally |

Native adapter/error exports are documented in [types](session-testing-types.md). All source exports covered; no missing re-exports. [Case coverage and proof ledger](session-testing-conformance.md).
