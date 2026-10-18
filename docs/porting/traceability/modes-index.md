# modes index traceability

Pinned source: `packages/coding-agent/src/modes/index.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

The module only re-exports the mode surface; the Rust equivalent is `crates/pi-coding-agent/src/modes/mod.rs`.

| TypeScript export | Rust mapping |
|---|---|
| `InteractiveMode`, `InteractiveModeOptions` | **gap**: owned by the interactive-mode slice (`modes/interactive/`) |
| `JsonAgentSessionEvent` | no distinct type: `to_json_event` returns `serde_json::Value`, which is the wire contract |
| `PrintModeOptions`, `runPrintMode` | `print_mode::PrintModeOptions`, `run_print_mode` |
| `ModelInfo` (re-export from `rpc-client.ts`) | `pi_ai::types::Model` (the client returns full models, see `rpc-client.md`) |
| `RpcClient`, `RpcClientOptions`, `RpcCommandBody`, `RpcEventListener` | `rpc::RpcClient`, `RpcClientOptions`, `RpcCommandBody`; the listener is a closure accepted by `on_event` |
| `runRpcMode` | `rpc::run_rpc_mode` |
| `RpcCommand`, `RpcExtensionUIRequest`, `RpcExtensionUIResponse`, `RpcResponse`, `RpcSessionState` | `rpc::rpc_types` (re-exported) |

No executable behavior exists in this module, so no differential applies.
