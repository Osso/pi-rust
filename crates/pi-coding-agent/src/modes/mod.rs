//! Port of `packages/coding-agent/src/modes/index.ts`.

pub mod extension_actions;
pub mod interactive;
pub mod json_event;
pub mod print_mode;
pub mod rpc;

pub use json_event::to_json_event;
pub use print_mode::{PrintMode, PrintModeOptions, run_print_mode};
pub use rpc::{
    CancelledResult, CycleModelResult, CycleThinkingLevelResult, EntriesResult, ExportHtmlResult, ForkMessage,
    ForkResult, RpcClient, RpcClientError, RpcClientOptions, RpcCommand, RpcCommandBody, RpcExtensionUiRequest,
    RpcExtensionUiResponse, RpcResponse, RpcSessionState, TreeResult, run_rpc_mode,
};
