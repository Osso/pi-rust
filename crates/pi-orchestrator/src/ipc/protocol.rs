//! JSONL envelopes. Parsers intentionally cast without structural validation like TS.
pub use pi_coding_agent::{RpcCommand, RpcExtensionUiRequest, RpcExtensionUiResponse, RpcResponse};
use pi_js_compat::js_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type ProtocolMessage = Value;
pub type OrchestratorResponse = Value;
pub type RpcServerMessage = Value;
pub type RpcClientMessage = Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OrchestratorRequest {
    Spawn {
        cwd: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    List,
    Stop {
        #[serde(rename = "instanceId")]
        instance_id: String,
    },
    Status {
        #[serde(rename = "instanceId")]
        instance_id: String,
    },
    Rpc {
        #[serde(rename = "instanceId")]
        instance_id: String,
        command: RpcCommand,
    },
    RpcStream {
        #[serde(rename = "instanceId")]
        instance_id: String,
    },
}

pub fn encode_message(value: &Value) -> String {
    format!("{}\n", js_json::stringify(value))
}
pub fn parse_request_line(line: &str) -> Result<Value, String> {
    js_json::parse(line).map_err(|e| e.to_string())
}
pub fn parse_response_line(line: &str) -> Result<Value, String> {
    parse_request_line(line)
}
pub fn error_response(error: impl Into<String>) -> Value {
    serde_json::json!({"type":"error","ok":false,"error":error.into()})
}
