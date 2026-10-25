mod jsonl;
pub mod rpc_client;
pub mod rpc_mode;
pub mod rpc_types;

pub use jsonl::{JsonlLineDecoder, JsonlLineReader, attach_jsonl_line_reader, serialize_json_line};
pub use rpc_client::{
    CancelledResult, CycleModelResult, CycleThinkingLevelResult, DEFAULT_EVENT_TIMEOUT, EntriesResult,
    ExportHtmlResult, ForkMessage, ForkResult, ModelInfo, RpcClient, RpcClientError, RpcClientOptions,
    RpcEventListener, RpcEventSubscription, TreeResult,
};
pub use rpc_mode::run_rpc_mode;
pub use rpc_types::{
    ExtensionUiRequestTag, ExtensionUiResponseTag, NotifyType, QueueMode, RpcCommand, RpcCommandBody,
    RpcExtensionUiRequest, RpcExtensionUiRequestPayload, RpcExtensionUiResponse, RpcExtensionUiResponsePayload,
    RpcResponse, RpcResponseTag, RpcSessionState, RpcSlashCommand, RpcSlashCommandSource, StreamingBehavior,
    WidgetPlacement,
};
