# AgentToolExecutionContext traceability

Pinned source: `packages/agent/src/types.ts:387-393` and package export `packages/agent/src/index.ts:145` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `types.ts:387-393` | Public mutable interface with required `startedAt: number` and optional `agentId?: string` fields in source order | `harness::types::AgentToolExecutionContext` uses public mutable `started_at: f64` and `agent_id: Option<String>` fields in source order; focused tests cover mutation, optional states, binary64 values, and representable strings |
| `index.ts:145` | `export * from "./types.ts"` exposes the interface from the agent package | `pi_agent_core::harness::AgentToolExecutionContext` re-exports the same Rust record; public-index typeproof remains pending |
| Source comments on `startedAt` and `agentId` | Describe lifecycle meaning for producers; they do not add value-level fields or validation | Rust ports the record shape only. Timestamp capture, identity preparation, producers, and lifecycle behavior remain excluded |
| Differential requests `13168..=13175` | Eight source-shaped request groups compare optional state, strings, and unrestricted numeric values | Differential fixture and oracle support are pending; no runtime constructor or orchestration proof is claimed |

## Rust boundary

- `AgentToolExecutionContext` derives `Debug` only.
- TypeScript `number` maps to unrestricted Rust `f64`; no validation, normalization, or integer narrowing is added.
- Missing `agentId` maps to `None`; present representable strings map to `Some(String)`.
- Rust strings preserve representable Unicode scalars, NUL, punctuation, empty values, and embedded/trailing newlines. Invalid JavaScript runtime values and lone UTF-16 surrogates remain outside the typed Rust boundary.
- No dependency change is required.

## Focused proof

- Focused RED: `/tmp/pi-rust-agent-tool-execution-context-red.out` records `cargo test -p pi-agent-core --test agent_tool_execution_context` exit 101 in a clean temporary worktree before production changes, with exactly two unresolved imports: the harness-root type and the module type.
- Focused GREEN: `/tmp/pi-rust-agent-tool-execution-context-green.out` records the five focused tests passing: public mutation, module/root paths, optional states, binary64 bits, and Unicode/NUL/newline strings.
- Targeted Clippy: `/tmp/pi-rust-agent-tool-execution-context-clippy.out` records `cargo clippy -p pi-agent-core --test agent_tool_execution_context -- -D warnings` exit 0.
- Formatting and diff: `/tmp/pi-rust-agent-tool-execution-context-fmt.out` and `/tmp/pi-rust-agent-tool-execution-context-diff.out` record exit 0.
- Changed-Rust readability: `/tmp/pi-rust-agent-tool-execution-context-readability-agent_tool_execution_context.rs.json` and the corresponding module/type artifacts report no threshold violations.

## Pending proof

- Add exactly eight persistent source-shaped differential groups `13168..=13175` after request `13167`; first request must provide the valid unsupported-operation RED boundary before oracle support.
- Add a strict pinned public-index typeproof for `AgentToolExecutionContext`, proving exact fields, mutability, optionality, and `number` identity without proving producers or lifecycle behavior.
- Run the differential/oracle and Node syntax gates, then final workspace tests, workspace check, all-feature Clippy with `-D warnings`, formatting, diff, readability, documentation, dependency, clean-pin, and inherited verification gates.

## Exclusions

AgentTool callbacks/results, external content, provider `Usage`, generic details, producers, lifecycle/timing validation, execution, scheduling, orchestration, streams, cancellation, sessions, providers, models, constructors, parser/serde behavior, defaults, fallbacks, invalid runtime inputs, protocol behavior, coding-agent behavior, and dependencies remain out of scope.
