//! Persistent records from orchestrator/types.ts.
use pi_js_compat::js_json::JsString;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceStatus {
    Starting,
    Online,
    Stopping,
    Stopped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineRecord {
    pub id: JsString,
    pub created_at: JsString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<JsString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceRecord {
    pub id: JsString,
    pub status: InstanceStatus,
    pub cwd: JsString,
    pub created_at: JsString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<JsString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<JsString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<JsString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_file: Option<JsString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius_pi_id: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadiusRegistration {
    pub heartbeat_interval_ms: u64,
    pub expires_in_ms: u64,
}
