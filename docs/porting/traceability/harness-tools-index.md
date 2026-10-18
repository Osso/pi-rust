# Harness tools index traceability

Pinned source: `packages/agent/src/harness/tools/index.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/tools/mod.rs`.

| TypeScript source | Observable contract | Rust mapping |
|---|---|---|
| `index.ts:2-11` | Export bash execution/prepare/details/input/options and factory | `BashExecution`, `BashPrepare`, `BashTool`, `BashToolOptions`; details/input are JSON `Value` |
| `index.ts:12-16` | Export edit factory, details, input | `EditTool`; details/input are JSON `Value` |
| `index.ts:17-25` | Export read factory, image processor/result, details/input/options | `ReadTool`, `ReadImageProcessor`, `ReadImageProcessorResult`, `ReadToolOptions`; details/input are JSON `Value` |
| `index.ts:26` | Export `ExecutionToolContext` | `ExecutionToolContext` |
| `index.ts:27` | Export write factory and input | `WriteTool`; input is a JSON `Value` |

## Rust boundary

Input and details records are validated/serialized as `serde_json::Value` rather than separate structs; the schema and tool tests pin their observable JSON shape.
