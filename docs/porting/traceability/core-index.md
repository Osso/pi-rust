# core/index.ts → core barrel

Frozen `4f9e6f8a4`, 101 lines. Rust `crates/pi-coding-agent-core/src/core/mod.rs`.

| Frozen export group | Native mapping | Proof |
| --- | --- | --- |
| AgentSession/config/events/model cycle/prompt/stats | direct agent_session reexports | existing session consumers |
| runtime factory/result/create/runtime | direct agent_session_runtime reexports | existing runtime consumers |
| services/options/diagnostics/create functions | direct agent_session_services reexports | SDK services tests |
| BashExecutorOptions/BashResult/executeBashWithOperations | direct bash_executor forwards (previously missing) | core_and_sdk_barrels_forward_frozen_public_contracts |
| CompactionResult, event bus and experimental feature query | direct forwards | compile-time consumers and owning-module proofs |
| extension API/events/tools/commands/factory/runner/UI/result types | direct extension forwards; Handler exported as ExtensionHandler | same barrel test; real tool result serialized through core API |
| BuildSystemPromptOptions | direct system_prompt forward | extension barrel test |
| agent activity/lifecycle/mailbox/node/snapshot/result/spawn/steering types and MultiAgentStore | direct native store forwards | existing store consumers |
| createSyntheticSourceInfo | direct source_info forward | owning source-info proof |

RED `/tmp/ext-api2-core-sdk-barrel-red.log`: source-facing core/SDK imports missing or private. GREEN in the current `extension_session_read_view::core_and_sdk_barrels_forward_frozen_public_contracts` test; see [closure proof](../ext-api-repin-proof.md).

Gap: erased AgentCommandResult union has no exact standalone native type; native store commands expose their existing native result enums. No unused carrier is added merely for this type-only export. No runtime export is missing for this gap.
