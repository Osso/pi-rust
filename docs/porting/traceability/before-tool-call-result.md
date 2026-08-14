# BeforeToolCallResult traceability

Pinned source: `packages/agent/src/types.ts:55-69` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`; interface declaration `61-69`, package export `packages/agent/src/index.ts:145`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `types.ts:61-63` | Public mutable optional `block?: boolean` and `reason?: string` fields | `harness::types::BeforeToolCallResult` uses `pub block: Option<bool>` and `pub reason: Option<String>`; focused tests cover omission, both booleans, empty/control/unicode strings, combinations, and mutation. |
| `types.ts:68` | Public mutable optional `terminate?: boolean` field | Rust uses `pub terminate: Option<bool>` in source order; focused tests cover omission, both booleans, combinations, and mutation. |
| `index.ts:145` | `export * from "./types.ts"` exposes the interface from the agent package | Rust re-exports from `pi_agent_core::harness`; the module path is `pi_agent_core::harness::types::BeforeToolCallResult`; no crate-root export is added. |
| `types.ts:55-60,64-67` | Comments describe blocking/error/default-message and batch-termination behavior for consumers | Context only. No hook execution, error generation, default selection, batch termination, or orchestration behavior is ported. |
| Differential request `13180` | Interface is erased at runtime; ordinary source-shaped records retain field presence and values | Eleven state-tag fixtures compare JSON-shaped records through the shared oracle. The proof does not claim runtime interface identity, excess-property identity, or omission/`undefined` identity. |
| Pinned compiler options | `exactOptionalPropertyTypes` is disabled; explicit `undefined` assignments are accepted | Typeproof verifies this option, exact optional writable properties, valid partial/full records, explicit `undefined`, and the fresh excess-property diagnostic. Rust `None` is the typed absence state. |

## Proof artifacts

- Focused RED: `/tmp/pi-rust-before-tool-call-result-focused-red.out` at base `44737ce`, exit 101, with only missing module/harness-root exports.
- Focused GREEN/static/readability: `/tmp/pi-rust-before-tool-call-result-focused-green.out`, `/tmp/pi-rust-before-tool-call-result-focused-clippy.out`, `/tmp/pi-rust-before-tool-call-result-focused-fmt.out`, `/tmp/pi-rust-before-tool-call-result-focused-diff.out`, and `/tmp/pi-rust-before-tool-call-result-focused-readability.out`.
- Differential RED: `/tmp/pi-rust-before-tool-call-result-differential-red.out` at `77b0580`, exit 101 with structured unsupported operation for request `13180`.
- Differential GREEN: `/tmp/pi-rust-before-tool-call-result-differential-green.out` at `f2badfb`, exit 0.
- Aggregate oracle/typeproof: `/tmp/pi-rust-before-tool-call-result-oracle.out`.
- Dedicated public-index typeproof: `/tmp/pi-rust-before-tool-call-result-typeproof.out`; compiler 5.9.3, real-index wildcard resolution, exact field/order/optional/mutable proof, explicit `undefined`, and excess-property diagnostic.
- Targeted readability: `/tmp/pi-rust-before-tool-call-result-readability.out`.

Production commit: `77b0580`. Differential/typeproof commit: `f2badfb`.

## Rust boundary and exclusions

`BeforeToolCallResult` derives `Debug` only. No `Clone`, equality, `Default`, constructors, serde, validation, or conversion traits are added. Public fields remain mutable and preserve source order.

The typed Rust boundary excludes arbitrary JavaScript runtime values, `null`, excess properties, object identity, and lone UTF-16 surrogates. JSON fixtures omit `None` fields and never emit `null`. Explicit JavaScript `undefined` is proven only in TypeScript; Rust does not distinguish it from omission.

`beforeToolCall`, `BeforeToolCallContext`, `AfterToolCallResult`, blocking/error behavior, default messages, termination logic, callbacks, agent loops, tools, providers, messages, sessions, serialization, and orchestration remain unported. No dependency, deployment, protocol, or unrelated API change applies.

Final workspace, static, dependency/lock, clean-pin, and inherited verification remain pending.
