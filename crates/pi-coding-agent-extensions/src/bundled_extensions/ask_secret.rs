//! Bundled `ask-secret` extension at the common source pin.

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::sync::{Arc, OnceLock};

use pi_agent_core::agent::types::{AgentToolResult, AgentToolUpdateCallback};
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{self, JsString, JsValue, encode_node_utf8};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::AsyncWriteExt;

use crate::core::extensions::types::{
    BoxFuture, ExtensionApi, ExtensionContext, ExtensionMode, ExtensionUiDialogOptions, HandlerError, ToolDefinition,
};
use crate::utils::paths::{node_basename, node_dirname, node_join};

const AUTH_SUDO_EXECUTABLE: &str = "authsudo";
const SECRETS_BROKER_ADMIN: &str = "/usr/bin/secrets-broker-admin";
const PROVISION_OPERATION: &str = "provision-browser";
const DIRECTORY_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskSecretBrowserField {
    #[serde(rename = "type")]
    pub field_type: JsString,
    pub name: JsString,
    pub selector: JsString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskSecretProvisionRequest {
    pub record: JsString,
    pub fields: Vec<AskSecretBrowserField>,
}

pub type AskSecretProvisioner = Arc<
    dyn Fn(
            AskSecretProvisionRequest,
            Vec<JsString>,
            Option<AbortSignal>,
        ) -> BoxFuture<'static, Result<(), HandlerError>>
        + Send
        + Sync,
>;
pub type AskSecretRenameFile =
    Arc<dyn Fn(JsString, JsString) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;

#[derive(Clone, Default)]
pub struct AskSecretToolOptions {
    pub provision: Option<AskSecretProvisioner>,
    pub rename_file: Option<AskSecretRenameFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskSecretDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<JsString>,
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

enum SecretRequest {
    Browser(AskSecretProvisionRequest),
    File { path: JsString, label: JsString },
}

pub fn ask_secret_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_tool(create_ask_secret_tool_definition(AskSecretToolOptions::default()))
        .map_err(HandlerError::from)
}

pub fn create_ask_secret_tool_definition(options: AskSecretToolOptions) -> ToolDefinition {
    let provision = options.provision.unwrap_or_else(|| {
        Arc::new(|request, values, signal| Box::pin(provision_browser_credential(request, values, signal)))
    });
    let rename_file = options.rename_file.unwrap_or_else(|| {
        Arc::new(|source, destination| {
            Box::pin(async move {
                tokio::fs::rename(encode_node_utf8(&source), encode_node_utf8(&destination))
                    .await
                    .map_err(|error| {
                        let mut record = pi_js_compat::node_fs::node_fs_error_record(&error, "rename", &source);
                        record.message =
                            pi_js_compat::node_fs::node_fs_dest_error(&error, "rename", &source, &destination);
                        HandlerError::from(
                            NativeNodeError::from(record).with_property("dest", JsValue::String(destination)),
                        )
                    })
            })
        })
    });
    ToolDefinition {
        name: "ask_secret".to_owned(),
        label: "Ask Secret".into(),
        description:
            "Prompt for browser credentials or one file-backed secret without returning secret values to the model."
                .into(),
        prompt_snippet: Some(
            "Prompt for browser credentials or a single file-backed secret through a hidden interactive prompt.".into(),
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
                let rename_file = Arc::clone(&rename_file);
                Box::pin(async move {
                    let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
                    execute_ask_secret(provision, rename_file, params, signal, ctx).await
                })
            },
        ),
        render_call: None,
        render_result: None,
    }
}

fn ask_secret_schema() -> serde_json::Value {
    json!({
        "type":"object",
        "properties":{
            "path":{"type":"string","description":"File secret: absolute destination path. Use with label, not record/fields."},
            "label":{"type":"string","description":"File secret: non-empty user-facing label for the masked secret prompt."},
            "record":{"type":"string","description":"Browser credentials: Secrets Broker record path. Use with fields, not path/label."},
            "fields":{"type":"array","items":{
                "type":"object","required":["type","name","selector"],
                "properties":{
                    "type":{"anyOf":[{"type":"string","const":"text"},{"type":"string","const":"email"},{"type":"string","const":"password"}]},
                    "name":{"type":"string","description":"Non-empty user-facing credential field name."},
                    "selector":{"type":"string","description":"CSS selector for the browser field."}
                },"additionalProperties":false
            },"minItems":1}
        },"additionalProperties":false
    })
}

async fn execute_ask_secret(
    provision: AskSecretProvisioner,
    rename_file: AskSecretRenameFile,
    params: JsValue,
    signal: Option<AbortSignal>,
    ctx: ExtensionContext,
) -> Result<AgentToolResult, HandlerError> {
    let request = parse_request(&params)?;
    validate_request(&request)?;
    if ctx.mode()? != ExtensionMode::Tui || !ctx.has_ui()? {
        return Ok(status_result(
            "Error: ask_secret requires an interactive TUI session",
            AskSecretStatus::Unavailable,
        ));
    }
    match request {
        SecretRequest::Browser(request) => execute_browser_request(provision, request, signal, ctx).await,
        SecretRequest::File { path, label } => execute_file_request(rename_file, path, label, signal, ctx).await,
    }
}

async fn execute_browser_request(
    provision: AskSecretProvisioner,
    request: AskSecretProvisionRequest,
    signal: Option<AbortSignal>,
    ctx: ExtensionContext,
) -> Result<AgentToolResult, HandlerError> {
    let mut values = Vec::new();
    for field in &request.fields {
        let value = ctx
            .ui()?
            .input(
                &field.name,
                Some(&pi_js_compat::js_format!("Enter {}", field.name)),
                Some(&ExtensionUiDialogOptions {
                    signal: signal.clone(),
                    timeout: None,
                    secret: (field.field_type == "password").then_some(true),
                }),
            )
            .await
            .map_err(HandlerError::from)?;
        let Some(value) = value else {
            return Ok(status_result(
                "Secret provisioning cancelled.",
                AskSecretStatus::Cancelled,
            ));
        };
        values.push(value);
    }
    provision(request.clone(), values, signal).await?;
    ask_secret_result(
        pi_js_compat::js_format!("Provisioned Secrets Broker record {}.", request.record),
        AskSecretDetails {
            path: None,
            record: Some(request.record),
            status: AskSecretStatus::Provisioned,
        },
    )
}

async fn execute_file_request(
    rename_file: AskSecretRenameFile,
    path: JsString,
    label: JsString,
    signal: Option<AbortSignal>,
    ctx: ExtensionContext,
) -> Result<AgentToolResult, HandlerError> {
    validate_file_destination(&path)?;
    let secret = ctx
        .ui()?
        .input(
            &label,
            Some(&"Enter secret".into()),
            Some(&ExtensionUiDialogOptions {
                signal: signal.clone(),
                timeout: None,
                secret: Some(true),
            }),
        )
        .await
        .map_err(HandlerError::from)?;
    let Some(secret) = secret else {
        return Ok(status_result(
            "Secret provisioning cancelled.",
            AskSecretStatus::Cancelled,
        ));
    };
    let mut payload = encode_node_utf8(&secret).into_bytes();
    payload.push(b'\n');
    let result = write_secret_payload_atomically(&path, &payload, rename_file, signal).await;
    payload.fill(0);
    result?;
    ask_secret_result(
        pi_js_compat::js_format!("Saved secret to {}.", path),
        AskSecretDetails {
            path: Some(path),
            record: None,
            status: AskSecretStatus::Provisioned,
        },
    )
}

fn parse_request(params: &JsValue) -> Result<SecretRequest, HandlerError> {
    let is_file = params.get("path").is_some() || params.get("label").is_some();
    let is_browser = params.get("record").is_some() || params.get("fields").is_some();
    if is_file == is_browser {
        return Err(HandlerError::new(
            "ask_secret takes either path and label, or record and fields",
        ));
    }
    if is_file {
        require_field(params, "path", "path is required with label")?;
        require_field(params, "label", "label is required with path")?;
        return Ok(SecretRequest::File {
            path: string_field(params, "path")?,
            label: string_field(params, "label")?,
        });
    }
    require_field(params, "record", "record is required with fields")?;
    require_field(params, "fields", "fields is required with record")?;
    let fields = params
        .get("fields")
        .and_then(JsValue::as_array)
        .ok_or_else(|| HandlerError::new("fields must be an array"))?
        .iter()
        .map(|field| {
            Ok(AskSecretBrowserField {
                field_type: field
                    .get("type")
                    .and_then(JsValue::as_string)
                    .cloned()
                    .unwrap_or_default(),
                name: string_field(field, "name")?,
                selector: string_field(field, "selector")?,
            })
        })
        .collect::<Result<Vec<_>, HandlerError>>()?;
    Ok(SecretRequest::Browser(AskSecretProvisionRequest {
        record: string_field(params, "record")?,
        fields,
    }))
}

fn require_field(params: &JsValue, field: &str, message: &str) -> Result<(), HandlerError> {
    if params.get(field).is_none() {
        return Err(HandlerError::new(message));
    }
    Ok(())
}

fn string_field(params: &JsValue, name: &str) -> Result<JsString, HandlerError> {
    params
        .get(name)
        .and_then(JsValue::as_string)
        .cloned()
        .ok_or_else(|| HandlerError::new(format!("{name} must be a string")))
}

fn validate_request(request: &SecretRequest) -> Result<(), HandlerError> {
    let request = match request {
        SecretRequest::File { path, label } => {
            validate_argument(path, "path")?;
            validate_argument(label, "label")?;
            if !path.starts_with("/") {
                return Err(HandlerError::new("path must be absolute"));
            }
            return Ok(());
        }
        SecretRequest::Browser(request) => request,
    };
    validate_argument(&request.record, "record")?;
    if request.fields.is_empty() {
        return Err(HandlerError::new("fields must not be empty"));
    }
    for (index, field) in request.fields.iter().enumerate() {
        if field.field_type != "text" && field.field_type != "email" && field.field_type != "password" {
            return Err(HandlerError::new(format!("fields[{index}].type is invalid")));
        }
        validate_argument(&field.name, &format!("fields[{index}].name"))?;
        validate_argument(&field.selector, &format!("fields[{index}].selector"))?;
    }
    if request.record.starts_with("/") || request.record.split('/').any(|segment| segment == "..") {
        return Err(HandlerError::new("record must be a relative Secrets Broker path"));
    }
    Ok(())
}

fn validate_argument(value: &JsString, name: &str) -> Result<(), HandlerError> {
    if value.trim().is_empty() {
        return Err(HandlerError::new(format!("{name} must not be empty")));
    }
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| Regex::new(r"\p{C}").expect("constant control character pattern"));
    if char::decode_utf16(value.units().iter().copied()).any(|decoded| match decoded {
        Ok(character) => pattern.is_match(&character.to_string()),
        Err(_) => true,
    }) {
        return Err(HandlerError::new(format!("{name} must not contain control characters")));
    }
    Ok(())
}

fn file_error(error: std::io::Error, operation: &str, path: &JsString) -> HandlerError {
    NativeNodeError::from(pi_js_compat::node_fs::node_fs_error_record(&error, operation, path)).into()
}

fn validate_file_destination(path: &JsString) -> Result<(), HandlerError> {
    match std::fs::symlink_metadata(encode_node_utf8(path)) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(HandlerError::new("path must not reference a symbolic link"))
        }
        Ok(metadata) if !metadata.is_file() => {
            Err(HandlerError::new("path must reference a regular file or not exist"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(file_error(error, "lstat", path)),
    }
}

fn write_and_sync_secret(path: &JsString, payload: &[u8]) -> Result<(), HandlerError> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(FILE_MODE)
        .open(encode_node_utf8(path))
        .map_err(|error| file_error(error, "open", path))?;
    file.set_permissions(std::fs::Permissions::from_mode(FILE_MODE))
        .map_err(|error| file_error(error, "fchmod", path))?;
    file.write_all(payload)
        .map_err(|error| file_error(error, "write", path))?;
    file.sync_all().map_err(|error| file_error(error, "fsync", path))
}

async fn write_secret_payload_atomically(
    path: &JsString,
    payload: &[u8],
    rename_file: AskSecretRenameFile,
    signal: Option<AbortSignal>,
) -> Result<(), HandlerError> {
    throw_if_aborted(signal.as_ref())?;
    let parent = node_dirname(path);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(DIRECTORY_MODE)
        .create(encode_node_utf8(&parent))
        .map_err(|error| file_error(error, "mkdir", &parent))?;
    validate_file_destination(path)?;
    let temporary = node_join(
        &parent,
        pi_js_compat::js_format!(
            ".{}.{}.tmp",
            node_basename(path),
            pi_ai::utils::random_uuid::random_uuid()?
        ),
    );
    let result = async {
        write_and_sync_secret(&temporary, payload)?;
        throw_if_aborted(signal.as_ref())?;
        validate_file_destination(path)?;
        rename_file(temporary.clone(), path.clone()).await
    }
    .await;
    let cleanup = match std::fs::remove_file(encode_node_utf8(&temporary)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(file_error(error, "unlink", &temporary)),
    };
    match (result, cleanup) {
        (Err(write), Err(cleanup)) => {
            let errors = [write, cleanup]
                .into_iter()
                .map(|error| pi_ai::request::CallbackError::from(error).thrown_value().value())
                .collect::<Vec<_>>();
            Err(NativeNodeError::from_constructor(
                NativeErrorClass::AggregateError,
                "Secret file write and cleanup failed",
            )
            .with_property("errors", JsValue::Array(errors))
            .into())
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

fn throw_if_aborted(signal: Option<&AbortSignal>) -> Result<(), HandlerError> {
    if signal.is_some_and(AbortSignal::aborted) {
        return Err(HandlerError::new("ask_secret cancelled"));
    }
    Ok(())
}

fn ask_secret_result(text: impl Into<JsString>, details: AskSecretDetails) -> Result<AgentToolResult, HandlerError> {
    Ok(AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(js_json::to_value(&details).map_err(|error| HandlerError::new(error.to_string()))?),
        is_error: None,
        terminate: None,
    })
}

fn status_result(text: &str, status: AskSecretStatus) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(pi_js_compat::js_json!({"status": match status {
            AskSecretStatus::Provisioned => "provisioned", AskSecretStatus::Cancelled => "cancelled", AskSecretStatus::Unavailable => "unavailable",
        }})),
        is_error: None,
        terminate: None,
    }
}

async fn provision_browser_credential(
    request: AskSecretProvisionRequest,
    values: Vec<JsString>,
    signal: Option<AbortSignal>,
) -> Result<(), HandlerError> {
    throw_if_aborted(signal.as_ref())?;
    let fields = js_json::to_value(&request.fields).map_err(|error| HandlerError::new(error.to_string()))?;
    let mut child = tokio::process::Command::new(AUTH_SUDO_EXECUTABLE)
        .args([
            "-u",
            "secrets-broker",
            SECRETS_BROKER_ADMIN,
            PROVISION_OPERATION,
            "--record",
        ])
        .arg(encode_node_utf8(&request.record))
        .arg("--fields-json")
        .arg(js_json::stringify_lossless(&fields))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| provisioning_failure(signal.as_ref()))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| HandlerError::new("Secrets Broker provisioning has no stdin"))?;
    let value = js_json::to_value(&values).map_err(|error| HandlerError::new(error.to_string()))?;
    let mut payload = js_json::stringify_lossless(&value).into_bytes();
    let operation = async {
        stdin.write_all(&payload).await.map_err(|_| ())?;
        stdin.shutdown().await.map_err(|_| ())?;
        drop(stdin);
        child.wait().await.map(|status| status.code()).map_err(|_| ())
    };
    let result = match signal.as_ref() {
        Some(signal) => tokio::select! { result = operation => result, _ = signal.wait() => Err(()) },
        None => operation.await,
    };
    payload.fill(0);
    throw_if_aborted(signal.as_ref())?;
    match result {
        Ok(Some(0)) => Ok(()),
        _ => Err(provisioning_failure(signal.as_ref())),
    }
}

fn provisioning_failure(signal: Option<&AbortSignal>) -> HandlerError {
    if signal.is_some_and(AbortSignal::aborted) {
        HandlerError::new("ask_secret cancelled")
    } else {
        HandlerError::new("Secrets Broker provisioning failed")
    }
}
