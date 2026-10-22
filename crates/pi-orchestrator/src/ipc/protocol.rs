//! JSONL envelopes. Parsers intentionally cast without structural validation like TS.
pub use pi_coding_agent::{RpcCommand, RpcExtensionUiRequest, RpcExtensionUiResponse, RpcResponse};
use pi_js_compat::{
    js_format,
    js_json::{self, JsString, JsValue as Value},
};
use serde::{Deserialize, Serialize};

pub type ProtocolMessage = Value;
pub type OrchestratorResponse = Value;
pub type RpcServerMessage = Value;
pub type RpcClientMessage = Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OrchestratorRequest {
    Spawn {
        cwd: JsString,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<JsString>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider: Option<JsString>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<JsString>,
    },
    List,
    Stop {
        #[serde(rename = "instanceId")]
        instance_id: JsString,
    },
    Status {
        #[serde(rename = "instanceId")]
        instance_id: JsString,
    },
    Rpc {
        #[serde(rename = "instanceId")]
        instance_id: JsString,
        command: RpcCommand,
    },
    RpcStream {
        #[serde(rename = "instanceId")]
        instance_id: JsString,
    },
}

pub fn encode_message(value: &Value) -> JsString {
    js_format!("{}\n", js_json::stringify_lossless(value))
}
pub fn parse_request_line(line: &JsString) -> Result<Value, JsString> {
    js_json::parse_lossless(line).map_err(js_json::JsonParseError::into_message)
}
pub fn parse_response_line(line: &JsString) -> Result<Value, JsString> {
    parse_request_line(line)
}
pub fn error_response(error: impl Into<JsString>) -> Value {
    pi_js_compat::js_json!({"type":"error","ok":false,"error":error.into()})
}
