# extensions/index.ts

Frozen `4f9e6f8a4` (190 lines); native `crates/pi-coding-agent-tools/src/core/extensions/mod.rs` and `types.rs` public forwarding.

| Frozen exports | Native mapping / proof |
|---|---|
| createExtensionRuntime, discoverAndLoadExtensions, loadExtensionFromFactory, loadExtensions | runtime/loader exports; frozen runner fixture creates actual factories/registry |
| ExtensionRunner and handler aliases | runner re-export plus native callback aliases/Handler trait; frozen 13-scenario dispatch/context/registration differential |
| wrapRegisteredTool(s) | wrapper exports; 3 prepare/execute/cap/error tests |
| defineTool and is* tool guards | native types public functions and enum guards; existing extension registry/type tests |
| types.ts records/context/render/provider/tool/event aliases | native `pub use types::*`; per-tool JS narrowed aliases use the single native ToolCallEvent/ToolResultEvent and tool schemas, not duplicate structs |
| AgentToolResult, AgentToolUpdateCallback, ToolExecutionMode, SourceInfo, SlashCommandInfo/Source, ExecOptions/Result, BuildSystemPromptOptions, KeybindingsManager | public forwarding repaired in types.rs; external integration compile + concrete AgentToolResult serialization |
| ExtensionFactory | loader factory trait object; Rust-native extension approval |
| AppKeybinding / generic ExtensionHandler | native key strings/KeyId and typed Handler trait, rather than JS string union/generic function alias |

RED: nine required native forwarded types were unavailable through core::extensions (`/tmp/ext-api-barrel-red.log`). New external compile test covers these symbols; runtime serialized tool-result value stays exact. Frozen runner differential GREEN `/tmp/ext-api-runner-frozen.log` at 1dcf30994. No new type wrappers or dependencies.

Gaps: TS module loading and purely JS alias/generic syntax follow the Rust-native API approval; behavior is covered in each owning module's trace.
