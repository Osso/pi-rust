//! Frozen IPC dispatch, deliberately ignoring spawn provider/model like the source.
use crate::{
    ipc::{
        protocol::error_response,
        server::{IpcRequestHandler, OnMessage, RpcStream},
    },
    supervisor::OrchestratorSupervisor,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::{
    js_format, js_json as json,
    js_json::{JsString, JsValue as Value},
};
use std::sync::Arc;

pub fn instance_summary(instance: &Value) -> Value {
    let mut summary = json!({"id":instance["id"],"status":instance["status"],"cwd":instance["cwd"]});
    crate::radius::copy_optional(
        instance,
        &mut summary,
        &["label", "sessionId", "sessionFile", "radiusPiId"],
    );
    summary
}
fn unknown_instance(id: &JsString) -> Value {
    error_response(js_format!("Unknown instance: {}", id))
}
pub async fn handle_ipc_request(supervisor: &Arc<OrchestratorSupervisor>, request: Value) -> Result<Value, JsString> {
    let id = request["instanceId"].as_string();
    match request["type"].as_string() {
        Some(tag) if tag == "spawn" => {
            let instance = supervisor
                .spawn_instance(
                    request["cwd"].as_string().ok_or("spawn cwd missing")?.into(),
                    request["label"].as_string().map(JsString::from),
                )
                .await?;
            Ok(json!({"type":"spawn_result","ok":true,"instance":instance_summary(&instance)}))
        }
        Some(tag) if tag == "list" => Ok(
            json!({"type":"list_result","ok":true,"instances":supervisor.list_instances()?.iter().map(instance_summary).collect::<Vec<_>>()}),
        ),
        Some(tag) if tag == "status" || tag == "rpc_stream" => {
            let id = id.ok_or("instanceId missing")?;
            let Some(instance) = supervisor.get_instance(id)? else {
                return Ok(unknown_instance(id));
            };
            Ok(
                json!({"type":if request["type"]==json!("status"){"status_result"}else{"rpc_ready"},"ok":true,"instance":instance_summary(&instance)}),
            )
        }
        Some(tag) if tag == "stop" => {
            let id = id.ok_or("instanceId missing")?;
            if supervisor.stop_instance(id).await?.is_none() {
                return Ok(unknown_instance(id));
            }
            Ok(json!({"type":"stop_result","ok":true,"instanceId":id}))
        }
        Some(tag) if tag == "rpc" => {
            let id = id.ok_or("instanceId missing")?;
            let Some(response) = supervisor.handle_rpc(id, request["command"].clone()).await? else {
                return Ok(unknown_instance(id));
            };
            Ok(json!({"type":"rpc_result","ok":true,"response":response}))
        }
        _ => Err("unknown orchestrator request type".into()),
    }
}
pub struct OrchestratorHandler(pub Arc<OrchestratorSupervisor>);
impl IpcRequestHandler for OrchestratorHandler {
    fn handle(&self, request: Value) -> BoxFuture<'_, Result<Value, JsString>> {
        Box::pin(handle_ipc_request(&self.0, request))
    }
    fn open_rpc_stream(&self, id: &JsString, on_message: OnMessage) -> Option<Arc<dyn RpcStream>> {
        let stream = self.0.open_rpc_stream(id, on_message.clone(), on_message.clone())?;
        Some(Arc::new(HandlerStream { stream, on_message }))
    }
}

struct HandlerStream {
    stream: Arc<crate::supervisor::SupervisorStream>,
    on_message: OnMessage,
}
impl RpcStream for HandlerStream {
    fn handle_request(&self, request: Value) -> BoxFuture<'_, Result<(), JsString>> {
        Box::pin(async move {
            if request["type"] == json!("extension_ui_response") {
                return self.stream.handle_ui_response(request).await;
            }
            let response = self.stream.handle_rpc(request).await?;
            (self.on_message)(response);
            Ok(())
        })
    }
    fn close(&self) {
        self.stream.close();
    }
}
