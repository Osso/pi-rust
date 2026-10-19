# Supervisor response tool traceability

Pinned source: `packages/coding-agent/src/supervisor/response-tool.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/supervisor/response_tool.rs`.

Differential: `tests/differential_supervisor_response.rs` compares metadata (name, label, description, prompt snippet/guidelines, TypeBox parameters schema, execution mode) and execution results for seven parameter objects against the real module.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `response-tool.ts:5` | Tool name `supervisor_response` | `SUPERVISOR_RESPONSE_TOOL_NAME`, metadata request `28410` |
| `response-tool.ts:7-25` | Parameter schema: `kind` union of nine literals, five optional described strings | Metadata request `28410` (byte-equal JSON schema) |
| `response-tool.ts:27-43` | Label, description, prompt snippet/guidelines, `executionMode: "sequential"` | Metadata request `28410` |
| `response-tool.ts:44-53` | Advisory answers echo the untrimmed answer when non-blank; all other kinds return `Supervisor response recorded.`; details are the params; `terminate: true` | Execution requests `28420..=28426` |

## Semantics

- `execute` needs an `ExtensionContext` that the tool ignores; the testable text rule lives in `supervisor_response_text`, and the tool closure wraps it into `AgentToolResult`.
- The schema is a hand-written JSON value; the differential pins it to TypeBox output.

## Gaps

- The execute closure is exercised end-to-end by `tests/differential_supervisor_request_loop.rs`, which runs the real session and the real response tool; the direct metadata differential remains separate.

## Discrepancies

None found.
