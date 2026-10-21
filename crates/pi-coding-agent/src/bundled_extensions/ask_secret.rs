//! Bundled `ask-secret` extension (`extensions/ask-secret/src/index.ts`).

use std::sync::{Arc, OnceLock};

use pi_agent_core::agent::types::{AgentToolResult, AgentToolUpdateCallback};
use pi_ai::types::{TextContent, TextOrImageContent};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

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
    pub record: String,
    pub username_selector: String,
    pub password_selector: String,
}

pub type AskSecretProvisioner = Arc<
    dyn Fn(
            AskSecretProvisionRequest,
            String,
            String,
            Option<CancellationToken>,
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
    pub record: Option<String>,
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
        .map_err(HandlerError::new)
}

pub fn create_ask_secret_tool_definition(options: AskSecretToolOptions) -> ToolDefinition {
    let provision = options.provision.unwrap_or_else(|| {
        Arc::new(|request, username, password, signal| {
            Box::pin(provision_browser_credential(request, username, password, signal))
        })
    });

    ToolDefinition {
        name: "ask_secret".to_owned(),
        label: "Ask Secret".to_owned(),
        description: "Prompt for browser credentials and provision them directly into the Secrets Broker without returning secret values.".to_owned(),
        prompt_snippet: Some(
            "Provision browser credentials through a hidden interactive prompt and the typed Secrets Broker path."
                .to_owned(),
        ),
        prompt_guidelines: None,
        approval_required: Some(true),
        approval_kind: None,
        parameters: ask_secret_schema(),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: Some(pi_agent_core::harness::types::ToolExecutionMode::Sequential),
        execute: Arc::new(
            move |_tool_call_id: String,
                  params: serde_json::Value,
                  signal: Option<CancellationToken>,
                  _on_update: Option<AgentToolUpdateCallback>,
                  ctx: ExtensionContext| {
                let provision = Arc::clone(&provision);
                Box::pin(async move { execute_ask_secret(provision, params, signal, ctx).await })
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
    params: serde_json::Value,
    signal: Option<CancellationToken>,
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
            "Secrets Broker username",
            Some("Enter username"),
            Some(&ExtensionUiDialogOptions {
                signal: signal.clone(),
                timeout: None,
                secret: None,
            }),
        )
        .await
        .map_err(HandlerError::new)?;
    let Some(username) = username else {
        return Ok(cancelled_result());
    };
    let password = ctx
        .ui()?
        .input(
            "Secrets Broker password",
            Some("Enter password"),
            Some(&ExtensionUiDialogOptions {
                signal: signal.clone(),
                timeout: None,
                secret: Some(true),
            }),
        )
        .await
        .map_err(HandlerError::new)?;
    let Some(password) = password else {
        return Ok(cancelled_result());
    };

    provision(request.clone(), username, password, signal).await?;

    Ok(ask_secret_result(
        &format!("Provisioned Secrets Broker record {}.", request.record),
        AskSecretDetails {
            record: Some(request.record),
            status: AskSecretStatus::Provisioned,
        },
    ))
}

fn parse_request(params: &serde_json::Value) -> Result<AskSecretProvisionRequest, HandlerError> {
    let record = string_field(params, "record")?;
    let username_selector = string_field(params, "usernameSelector")?;
    let password_selector = string_field(params, "passwordSelector")?;
    Ok(AskSecretProvisionRequest {
        record,
        username_selector,
        password_selector,
    })
}

fn string_field(params: &serde_json::Value, name: &str) -> Result<String, HandlerError> {
    params
        .get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| HandlerError::new(format!("{name} must be a string")))
}

fn validate_request(request: &AskSecretProvisionRequest) -> Result<(), HandlerError> {
    validate_argument(&request.record, "record")?;
    validate_argument(&request.username_selector, "usernameSelector")?;
    validate_argument(&request.password_selector, "passwordSelector")?;
    if request.record.starts_with('/') || request.record.split('/').any(|segment| segment == "..") {
        return Err(HandlerError::new("record must be a relative Secrets Broker path"));
    }
    Ok(())
}

fn validate_argument(value: &str, name: &str) -> Result<(), HandlerError> {
    if value.trim().is_empty() {
        return Err(HandlerError::new(format!("{name} must not be empty")));
    }
    if control_character_pattern().is_match(value) {
        return Err(HandlerError::new(format!("{name} must not contain control characters")));
    }
    Ok(())
}

fn control_character_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"[^\P{C}\t]").expect("control character pattern"))
}

fn ask_secret_result(text: &str, details: AskSecretDetails) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(serde_json::to_value(details).expect("ask_secret details serialize")),
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
    username: String,
    password: String,
    signal: Option<CancellationToken>,
) -> Result<(), HandlerError> {
    if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(HandlerError::new("ask_secret cancelled"));
    }
    let mut command = tokio::process::Command::new(AUTH_SUDO_EXECUTABLE);
    command
        .arg("-u")
        .arg("secrets-broker")
        .arg(SECRETS_BROKER_ADMIN)
        .arg(PROVISION_OPERATION)
        .arg("--record")
        .arg(&request.record)
        .arg("--username-selector")
        .arg(&request.username_selector)
        .arg("--password-selector")
        .arg(&request.password_selector)
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
    let payload = format!("{username}\n{password}\n");
    let write_result = async {
        stdin.write_all(payload.as_bytes()).await?;
        stdin.shutdown().await
    }
    .await;

    let outcome = match signal.clone() {
        Some(token) => {
            tokio::select! {
                () = token.cancelled() => {
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

fn provisioning_failure(signal: Option<&CancellationToken>) -> HandlerError {
    if signal.is_some_and(CancellationToken::is_cancelled) {
        HandlerError::new("ask_secret cancelled")
    } else {
        HandlerError::new("Secrets Broker provisioning failed")
    }
}

fn spawn_error_message(command: &str, error: &std::io::Error) -> String {
    node_spawn_error_message("spawn", command, error)
}
