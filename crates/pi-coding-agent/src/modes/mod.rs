//! Port of `packages/coding-agent/src/modes/index.ts`.

pub use pi_coding_agent_core::modes::extension_actions;
pub use pi_coding_agent_interactive::modes::interactive;
pub mod json_event;
pub mod print_mode;
pub mod rpc;

pub use interactive::{
    InteractiveMode, InteractiveModeOptions, InteractiveTuiHandle, InteractiveTuiOptions, Renderer,
    create_interactive_tui, run_interactive_mode,
};
pub use print_mode::{PrintMode, PrintModeOptions, run_print_mode};
pub use rpc::{
    CancelledResult, CycleModelResult, CycleThinkingLevelResult, EntriesResult, ExportHtmlResult, ForkMessage,
    ForkResult, ModelInfo, RpcClient, RpcClientError, RpcClientOptions, RpcCommand, RpcCommandBody, RpcEventListener,
    RpcExtensionUiRequest, RpcExtensionUiResponse, RpcResponse, RpcSessionState, TreeResult, run_rpc_mode,
};
