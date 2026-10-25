//! Bundled `ask-secret` extension (`extensions/ask-secret/src/index.ts`).

use std::sync::{Arc, OnceLock};

use pi_agent_core::agent::types::{AgentToolResult, AgentToolUpdateCallback};
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{self, JsString, JsValue, encode_node_utf8};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::AsyncWriteExt;

use crate::utils::child_process::node_spawn_error_message;

use crate::core::extensions::types::{
    BoxFuture, ExtensionApi, ExtensionContext, ExtensionMode, ExtensionUiDialogOptions, HandlerError, ToolDefinition,
};

const AUTH_SUDO_EXECUTABLE: &str = "authsudo";
const SECRETS_BROKER_ADMIN: &str = "/usr/bin/secrets-broker-admin";
const PROVISION_OPERATION: &str = "provision-browser";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskSecretProvisionRequest {
    pub record: JsString,
    pub username_selector: JsString,
    pub password_selector: JsString,
}

pub type AskSecretProvisioner = Arc<
    dyn Fn(
            AskSecretProvisionRequest,
            JsString,
            JsString,
            Option<AbortSignal>,
        ) -> BoxFuture<'static, Result<(), HandlerError>>
        + Send
        + Sync,
>;

#[derive(Clone, Default)]
pub struct AskSecretToolOptions {
    pub provision: Option<AskSecretProvisioner>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskSecretDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<JsString>,
    pub status: AskSecretStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AskSecretStatus {
    Provisioned,
    Cancelled,
    Unavailable,
}

pub fn ask_secret_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_tool(create_ask_secret_tool_definition(AskSecretToolOptions::default()))
        .map_err(HandlerError::from)
}

pub fn create_ask_secret_tool_definition(options: AskSecretToolOptions) -> ToolDefinition {
    let provision = options.provision.unwrap_or_else(|| {
        Arc::new(|request, username, password, signal| {
            Box::pin(provision_browser_credential(request, username, password, signal))
        })
    });

    ToolDefinition {
        name: "ask_secret".to_owned(),
        label: "Ask Secret".into(),
        description: "Prompt for browser credentials and provision them directly into the Secrets Broker without returning secret values.".into(),
        prompt_snippet: Some(
            "Provision browser credentials through a hidden interactive prompt and the typed Secrets Broker path."
                .into(),
        ),
        prompt_guidelines: None,
        approval_required: Some(true),
        approval_kind: None,
        parameters: ask_secret_schema(),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: Some(pi_agent_core::harness::types::ToolExecutionMode::Sequential),
        execute: Arc::new(
            move |_tool_call_id: String,
                  params: JsValue,
                  signal: Option<AbortSignal>,
                  _on_update: Option<AgentToolUpdateCallback>,
                  ctx: Option<ExtensionContext>| {
                let provision = Arc::clone(&provision);
                Box::pin(async move {
            let ctx = crate::core::extensions::types::require_extension_context(ctx)?; execute_ask_secret(provision, params, signal, ctx).await })
            },
        ),
        render_call: None,
        render_result: None,
    }
}

fn ask_secret_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "required": ["record", "usernameSelector", "passwordSelector"],
        "properties": {
            "record": {"type": "string", "description": "Secrets Broker browser credential record path."},
            "usernameSelector": {"type": "string", "description": "CSS selector for the username field."},
            "passwordSelector": {"type": "string", "description": "CSS selector for the password field."}
        }
    })
}

async fn execute_ask_secret(
    provision: AskSecretProvisioner,
    params: JsValue,
    signal: Option<AbortSignal>,
    ctx: ExtensionContext,
) -> Result<AgentToolResult, HandlerError> {
    let request = parse_request(&params)?;
    validate_request(&request)?;
    if ctx.mode()? != ExtensionMode::Tui || !ctx.has_ui()? {
        return Ok(ask_secret_result(
            "Error: ask_secret requires an interactive TUI session",
            AskSecretDetails {
                record: None,
                status: AskSecretStatus::Unavailable,
            },
        ));
    }

    let username = ctx
        .ui()?
        .input(
            &"Secrets Broker username".into(),
            Some(&"Enter username".into()),
            Some(&ExtensionUiDialogOptions {
                signal: signal.clone(),
                timeout: None,
                secret: None,
            }),
        )
        .await
        .map_err(HandlerError::from)?;
    let Some(username) = username else {
        return Ok(cancelled_result());
    };
    let password = ctx
        .ui()?
        .input(
            &"Secrets Broker password".into(),
            Some(&"Enter password".into()),
            Some(&ExtensionUiDialogOptions {
                signal: signal.clone(),
                timeout: None,
                secret: Some(true),
            }),
        )
        .await
        .map_err(HandlerError::from)?;
    let Some(password) = password else {
        return Ok(cancelled_result());
    };

    provision(request.clone(), username, password, signal).await?;

    Ok(ask_secret_result(
        JsString::from("Provisioned Secrets Broker record ") + &request.record + ".",
        AskSecretDetails {
            record: Some(request.record),
            status: AskSecretStatus::Provisioned,
        },
    ))
}

fn parse_request(params: &JsValue) -> Result<AskSecretProvisionRequest, HandlerError> {
    let record = string_field(params, "record")?;
    let username_selector = string_field(params, "usernameSelector")?;
    let password_selector = string_field(params, "passwordSelector")?;
    Ok(AskSecretProvisionRequest {
        record,
        username_selector,
        password_selector,
    })
}

fn string_field(params: &JsValue, name: &str) -> Result<JsString, HandlerError> {
    params
        .get(name)
        .and_then(JsValue::as_string)
        .cloned()
        .ok_or_else(|| HandlerError::new(format!("{name} must be a string")))
}

fn validate_request(request: &AskSecretProvisionRequest) -> Result<(), HandlerError> {
    validate_argument(&request.record, "record")?;
    validate_argument(&request.username_selector, "usernameSelector")?;
    validate_argument(&request.password_selector, "passwordSelector")?;
    if request.record.starts_with("/") || request.record.split('/').any(|segment| segment == "..") {
        return Err(HandlerError::new("record must be a relative Secrets Broker path"));
    }
    Ok(())
}

fn validate_argument(value: &JsString, name: &str) -> Result<(), HandlerError> {
    if value.trim().is_empty() {
        return Err(HandlerError::new(format!("{name} must not be empty")));
    }
    if char::decode_utf16(value.units().iter().copied()).any(|decoded| match decoded {
        Ok(character) => control_character_pattern().is_match(&character.to_string()),
        Err(_) => true,
    }) {
        return Err(HandlerError::new(format!("{name} must not contain control characters")));
    }
    Ok(())
}

fn control_character_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"[^\P{C}\t]").expect("control character pattern"))
}

fn ask_secret_result(text: impl Into<JsString>, details: AskSecretDetails) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(
            js_json::to_value(&details)
                .expect("ask_secret details serialize")
                .into(),
        ),
        is_error: None,
        terminate: None,
    }
}

fn cancelled_result() -> AgentToolResult {
    ask_secret_result(
        "Secret provisioning cancelled.",
        AskSecretDetails {
            record: None,
            status: AskSecretStatus::Cancelled,
        },
    )
}

async fn provision_browser_credential(
    request: AskSecretProvisionRequest,
    username: JsString,
    password: JsString,
    signal: Option<AbortSignal>,
) -> Result<(), HandlerError> {
    if signal.as_ref().is_some_and(AbortSignal::aborted) {
        return Err(HandlerError::new("ask_secret cancelled"));
    }
    let mut command = tokio::process::Command::new(AUTH_SUDO_EXECUTABLE);
    command
        .arg("-u")
        .arg("secrets-broker")
        .arg(SECRETS_BROKER_ADMIN)
        .arg(PROVISION_OPERATION)
        .arg("--record")
        .arg(encode_node_utf8(&request.record))
        .arg("--username-selector")
        .arg(encode_node_utf8(&request.username_selector))
        .arg("--password-selector")
        .arg(encode_node_utf8(&request.password_selector))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| HandlerError::new(spawn_error_message(AUTH_SUDO_EXECUTABLE, &error)))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| HandlerError::new("Secrets Broker provisioning has no stdin"))?;
    let payload = encode_node_utf8(&(username + "\n" + password + "\n"));
    let write_result = async {
        stdin.write_all(payload.as_bytes()).await?;
        stdin.shutdown().await
    }
    .await;

    let outcome = match signal.clone() {
        Some(token) => {
            tokio::select! {
                _ = token.wait() => {
                    let _ = child.kill().await;
                    Err(true)
                }
                status = child.wait() => status.map(|status| status.code()).map_err(|_| false),
            }
        }
        None => child.wait().await.map(|status| status.code()).map_err(|_| false),
    };

    if write_result.is_err() {
        return Err(provisioning_failure(signal.as_ref()));
    }
    match outcome {
        Err(aborted) if aborted => Err(HandlerError::new("ask_secret cancelled")),
        Err(_) => Err(provisioning_failure(signal.as_ref())),
        Ok(code) if code != Some(0) => Err(provisioning_failure(signal.as_ref())),
        Ok(_) => Ok(()),
    }
}

fn provisioning_failure(signal: Option<&AbortSignal>) -> HandlerError {
    if signal.is_some_and(AbortSignal::aborted) {
        HandlerError::new("ask_secret cancelled")
    } else {
        HandlerError::new("Secrets Broker provisioning failed")
    }
}

fn spawn_error_message(command: &str, error: &std::io::Error) -> pi_js_compat::js_json::JsString {
    node_spawn_error_message("spawn", &pi_js_compat::js_json::JsString::from(command), error)
}
