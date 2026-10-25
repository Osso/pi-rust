# modes index traceability

Frozen: `packages/coding-agent/src/modes/index.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. All 21 lines are re-exports; no independent runtime branches.

| Frozen export | Native modes entrypoint |
| --- | --- |
| `InteractiveMode`, `InteractiveModeOptions` | Same names via `interactive` |
| `PrintModeOptions`, `runPrintMode` | `PrintModeOptions`, `run_print_mode` |
| `ModelInfo` | Actual four-field RPC `rpc_client::ModelInfo` record, not the unrelated full AI Model |
| `RpcClient`, `RpcClientOptions`, `RpcCommandBody`, `RpcEventListener` | Forwarded from native `rpc` module; listener receives exact `JsValue` and propagates callback errors |
| `runRpcMode` | `run_rpc_mode` |
| `RpcCommand`, `RpcExtensionUIRequest`, `RpcExtensionUIResponse`, `RpcResponse`, `RpcSessionState` | Same record/enum contracts with native Ui spelling |

`tests/modes_index.rs::public_modes_forward_rpc_model_and_listener_contracts` imports the modes surface externally, round-trips a concrete ModelInfo with unknown property, and delivers a concrete agent_end event through the public listener. RED `/tmp/settings-models-modes-red.log` proves missing ModelInfo/RpcEventListener re-exports. The intermediate rpc/mod.rs forwarding is mechanical cross-lane visibility, not a new algorithm/carrier. Existing native TUI helper and result-type exports remain implementation conveniences, not additional frozen behavior.

No runtime differential is required for a pure export module; source types are mapped above. Focused GREEN/clippy evidence follows the commit in the lane ledger.
