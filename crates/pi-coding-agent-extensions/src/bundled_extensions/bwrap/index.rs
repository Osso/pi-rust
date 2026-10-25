//! Port of `packages/coding-agent/extensions/bwrap/src/index.ts`.
//!
//! Deviation (approved 2026-10-18): the sandbox filesystem worker is the pi
//! executable's `--internal-bwrap-fs-worker` mode instead of Node
//! `fs-worker.cjs`; operations and protocol are identical.

use std::process::Stdio;
use std::sync::Arc;

use crate::utils::child_process::node_spawn_error_message;
use crate::utils::paths::{node_dirname, process_cwd};
use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::{
    js_json as json,
    js_json::{self, JsObject as Map, JsString, JsValue as Value, encode_node_utf8},
};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::core::extensions::loader::ExtensionFactory;
use crate::core::extensions::types::{
    BoxFuture, ExtensionApi, ExtensionContext, HandlerError, NotifyType, ToolDefinition, UserBashEvent,
    UserBashEventResult,
};
use crate::core::permissions::presets::SandboxProfileName;
use crate::core::tools::bash::create_bash_tool_definition;
use crate::core::tools::edit::{EditOperations, EditToolOptions, create_edit_tool_definition};
use crate::core::tools::find::{FindOperations, FindToolOptions, create_find_tool_definition};
use crate::core::tools::grep::{GrepToolOptions, create_grep_tool_definition};
use crate::core::tools::ls::{LsOperations, LsToolOptions, create_ls_tool_definition};
use crate::core::tools::read::{ReadOperations, ReadRange, ReadToolOptions, create_read_tool_definition};
use crate::core::tools::write::{WriteOperations, WriteToolOptions, create_write_tool_definition};
use crate::internal_runner::BWRAP_FS_WORKER_MODE;
use crate::modes::interactive::theme::ThemeColor;

use super::backend::BwrapInvocationOptions;
use super::backend::{
    BwrapSandboxProfile, build_bwrap_invocation, create_sandboxed_bash_operations, resolve_bwrap_sandbox_profile,
    run_bwrap_availability_check,
};

#[cfg(test)]
#[path = "operation_tests.rs"]
mod operation_tests;

const DEFAULT_BWRAP_COMMAND: &str = "bwrap";
const DEFAULT_GREP_LIMIT: f64 = 100.0;
const DEFAULT_MAX_BYTES: usize = 10 * 1024;

#[derive(Clone, Default)]
pub struct BwrapExtensionOptions {
    pub bwrap_command: Option<JsString>,
}

fn default_bwrap_command() -> JsString {
    pi_js_compat::process_env::get("PI_BWRAP_COMMAND")
        .map(JsString::from)
        .unwrap_or_else(|| DEFAULT_BWRAP_COMMAND.into())
}

fn read_sandbox_profile_name(ctx: &ExtensionContext) -> SandboxProfileName {
    ctx.settings_manager()
        .ok()
        .flatten()
        .and_then(|settings| {
            settings
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get_explicit_sandbox_profile()
        })
        .unwrap_or(SandboxProfileName::FullAccess)
}

fn read_sandbox_profile(ctx: &ExtensionContext) -> Option<BwrapSandboxProfile> {
    resolve_bwrap_sandbox_profile(read_sandbox_profile_name(ctx))
}

#[derive(Clone)]
struct SandboxParams {
    bwrap_command: JsString,
    cwd: JsString,
    profile: BwrapSandboxProfile,
}

fn read_sandbox_params(
    ctx: &ExtensionContext,
    bwrap_command: &JsString,
) -> Result<Option<SandboxParams>, HandlerError> {
    let Some(profile) = read_sandbox_profile(ctx) else {
        return Ok(None);
    };
    Ok(Some(SandboxParams {
        bwrap_command: bwrap_command.clone(),
        cwd: ctx.cwd()?,
        profile,
    }))
}

fn worker_path() -> Result<JsString, JsString> {
    std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned().into())
        .map_err(|error| error.to_string().into())
}

fn build_sandbox_worker_invocation(
    params: &SandboxParams,
    operation: &str,
    payload: &Map,
) -> Result<crate::bundled_extensions::bwrap::backend::BwrapInvocation, JsString> {
    let mut worker_payload = payload.clone();
    worker_payload.insert("workspace".into(), Value::String(params.cwd.clone().into()));
    let worker = worker_path()?;
    let command = vec![
        worker.clone(),
        BWRAP_FS_WORKER_MODE.into(),
        operation.into(),
        js_json::stringify_lossless(&Value::Object(worker_payload)).into(),
    ];
    let extra_read_only_paths = vec![node_dirname(&worker)];
    build_bwrap_invocation(&BwrapInvocationOptions {
        bwrap_command: params.bwrap_command.clone(),
        command,
        cwd: params.cwd.clone(),
        env: None,
        extra_read_only_paths: Some(extra_read_only_paths),
        home_dir: None,
        profile: params.profile,
    })
}

async fn run_json_in_sandbox(
    params: &SandboxParams,
    operation: &str,
    payload: Map,
    signal: Option<AbortSignal>,
) -> Result<Value, JsString> {
    run_bwrap_availability_check(&params.bwrap_command)?;
    let invocation = build_sandbox_worker_invocation(params, operation, &payload)?;
    let mut command = Command::new(encode_node_utf8(&invocation.command));
    command
        .args(invocation.argv.iter().map(encode_node_utf8))
        .env_clear()
        .envs(
            invocation
                .env
                .iter()
                .map(|(key, value)| (encode_node_utf8(key), encode_node_utf8(value))),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| node_spawn_error_message("spawn", &invocation.command, &error))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "sandbox stdout unavailable".to_owned())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "sandbox stderr unavailable".to_owned())?;
    let stdout_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    });
    let stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    });
    let signal_for_wait = signal.clone();
    let status = tokio::select! {
        status = child.wait() => status.map_err(|error| error.to_string())?,
        _ = async {
            match signal_for_wait {
                Some(signal) => { signal.wait().await; },
                None => std::future::pending::<()>().await,
            }
        } => {
            let _ = child.kill().await;
            return Err("Operation aborted".into());
        }
    };
    let stdout_bytes = stdout_task
        .await
        .map_err(|error| JsString::from(error.to_string()))?
        .map_err(|error| JsString::from(error.to_string()))?;
    let stderr_bytes = stderr_task
        .await
        .map_err(|error| JsString::from(error.to_string()))?
        .map_err(|error| JsString::from(error.to_string()))?;
    if signal.as_ref().is_some_and(AbortSignal::aborted) {
        return Err("Operation aborted".into());
    }
    let stdout_text = JsString::from(String::from_utf8_lossy(&stdout_bytes).into_owned());
    let stderr_text = JsString::from(String::from_utf8_lossy(&stderr_bytes).into_owned());
    if status.code() != Some(0) {
        let trimmed = stderr_text.trim();
        return Err(if trimmed.is_empty() {
            js_format!(
                "sandbox operation failed with exit code {}",
                status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "null".to_owned())
            )
        } else {
            trimmed.to_owned()
        }
        .into());
    }
    js_json::parse_lossless(&stdout_text).map_err(|error| error.into_message())
}

fn run_sandbox_operation<'a>(
    params: &'a SandboxParams,
    operation: &'a str,
    payload: Map,
    signal: Option<AbortSignal>,
) -> BoxFuture<'a, Result<Value, JsString>> {
    Box::pin(async move { run_json_in_sandbox(params, operation, payload, signal).await })
}

fn path_payload(path: &JsString) -> Map {
    let mut payload = Map::new();
    payload.insert("path".into(), Value::String(path.into()));
    payload
}

struct SandboxReadOperations {
    params: SandboxParams,
}

impl ReadOperations for SandboxReadOperations {
    fn read_file<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<Vec<u8>>> {
        Box::pin(async move {
            let value = run_sandbox_operation(&self.params, "readFile", path_payload(absolute_path), None)
                .await
                .map_err(crate::core::tools::node_fs::operation_error)?;
            Ok(pi_js_compat::base64::buffer_from_base64(
                value
                    .get("data")
                    .and_then(Value::as_string)
                    .cloned()
                    .unwrap_or_default(),
            ))
        })
    }

    fn read_range(&self) -> Option<ReadRange<'_>> {
        Some(Box::new(move |absolute_path, start, end| {
            Box::pin(async move {
                let mut payload = Map::new();
                payload.insert("end".into(), json!(end));
                payload.insert("path".into(), Value::String(absolute_path.into()));
                payload.insert("start".into(), json!(start));
                let value = run_sandbox_operation(&self.params, "readRange", payload, None)
                    .await
                    .map_err(crate::core::tools::node_fs::operation_error)?;
                Ok(pi_js_compat::base64::buffer_from_base64(
                    value
                        .get("data")
                        .and_then(Value::as_string)
                        .cloned()
                        .unwrap_or_default(),
                ))
            })
        }))
    }

    fn stat<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<Option<f64>>> {
        Box::pin(async move {
            let value = run_sandbox_operation(&self.params, "stat", path_payload(absolute_path), None)
                .await
                .map_err(crate::core::tools::node_fs::operation_error)?;
            Ok(value.get("size").and_then(Value::as_f64))
        })
    }

    fn access<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<()>> {
        Box::pin(async move {
            run_sandbox_operation(&self.params, "access", path_payload(absolute_path), None)
                .await
                .map(|_| ())
                .map_err(crate::core::tools::node_fs::operation_error)
        })
    }

    fn detect_image_mime_type<'a>(
        &'a self,
        absolute_path: &'a JsString,
    ) -> BoxFuture<'a, std::io::Result<Option<String>>> {
        Box::pin(async move { Ok(mime_type_for_path(absolute_path)) })
    }
}

fn mime_type_for_path(path: &JsString) -> Option<String> {
    let extension = crate::utils::paths::node_extname(path).to_lowercase();
    let mime_type = if extension == ".png" {
        "image/png"
    } else if extension == ".jpg" || extension == ".jpeg" {
        "image/jpeg"
    } else if extension == ".gif" {
        "image/gif"
    } else if extension == ".webp" {
        "image/webp"
    } else {
        return None;
    };
    Some(mime_type.to_owned())
}

struct SandboxWriteOperations {
    params: SandboxParams,
}

impl WriteOperations for SandboxWriteOperations {
    fn write_file<'a>(
        &'a self,
        absolute_path: &'a JsString,
        content: &'a JsString,
    ) -> BoxFuture<'a, std::io::Result<()>> {
        Box::pin(async move {
            let mut payload = Map::new();
            payload.insert("content".into(), Value::String(content.into()));
            payload.insert("path".into(), Value::String(absolute_path.into()));
            run_sandbox_operation(&self.params, "writeFile", payload, None)
                .await
                .map(|_| ())
                .map_err(crate::core::tools::node_fs::operation_error)
        })
    }

    fn mkdir<'a>(&'a self, directory: &'a JsString) -> BoxFuture<'a, std::io::Result<()>> {
        Box::pin(async move {
            run_sandbox_operation(&self.params, "mkdir", path_payload(directory), None)
                .await
                .map(|_| ())
                .map_err(crate::core::tools::node_fs::operation_error)
        })
    }
}

struct SandboxEditOperations {
    read: SandboxReadOperations,
    write: SandboxWriteOperations,
}

impl EditOperations for SandboxEditOperations {
    fn read_file<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<Vec<u8>>> {
        self.read.read_file(absolute_path)
    }

    fn write_file<'a>(
        &'a self,
        absolute_path: &'a JsString,
        content: &'a JsString,
    ) -> BoxFuture<'a, std::io::Result<()>> {
        self.write.write_file(absolute_path, content)
    }

    fn access<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<()>> {
        self.read.access(absolute_path)
    }
}

struct SandboxLsOperations {
    params: SandboxParams,
}

impl LsOperations for SandboxLsOperations {
    fn exists<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<bool>> {
        Box::pin(async move {
            let value = run_sandbox_operation(&self.params, "exists", path_payload(absolute_path), None)
                .await
                .map_err(crate::core::tools::node_fs::operation_error)?;
            Ok(value.get("exists").and_then(Value::as_bool).unwrap_or(false))
        })
    }

    fn is_directory<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<bool>> {
        Box::pin(async move {
            let value = run_sandbox_operation(&self.params, "stat", path_payload(absolute_path), None)
                .await
                .map_err(crate::core::tools::node_fs::operation_error)?;
            Ok(value.get("isDirectory").and_then(Value::as_bool).unwrap_or(false))
        })
    }

    fn readdir<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<Vec<JsString>>> {
        Box::pin(async move {
            let value = run_sandbox_operation(&self.params, "readdir", path_payload(absolute_path), None)
                .await
                .map_err(crate::core::tools::node_fs::operation_error)?;
            Ok(value
                .get("entries")
                .and_then(Value::as_array)
                .map(|entries| entries.iter().filter_map(Value::as_string).cloned().collect())
                .unwrap_or_default())
        })
    }
}

struct SandboxFindOperations {
    params: SandboxParams,
}

impl FindOperations for SandboxFindOperations {
    fn exists<'a>(&'a self, absolute_path: &'a JsString) -> BoxFuture<'a, std::io::Result<bool>> {
        Box::pin(async move {
            let value = run_sandbox_operation(&self.params, "exists", path_payload(absolute_path), None)
                .await
                .map_err(crate::core::tools::node_fs::operation_error)?;
            Ok(value.get("exists").and_then(Value::as_bool).unwrap_or(false))
        })
    }

    fn supports_glob(&self) -> bool {
        true
    }

    fn glob<'a>(
        &'a self,
        pattern: &'a JsString,
        cwd: &'a JsString,
        _ignore: &'a [JsString],
        limit: f64,
    ) -> BoxFuture<'a, std::io::Result<Vec<JsString>>> {
        Box::pin(async move {
            let mut payload = Map::new();
            payload.insert("cwd".into(), Value::String(cwd.into()));
            payload.insert("limit".into(), json!(limit));
            payload.insert("pattern".into(), Value::String(pattern.into()));
            let value = run_sandbox_operation(&self.params, "find", payload, None)
                .await
                .map_err(crate::core::tools::node_fs::operation_error)?;
            Ok(value
                .get("results")
                .and_then(Value::as_array)
                .map(|results| results.iter().filter_map(Value::as_string).cloned().collect())
                .unwrap_or_default())
        })
    }
}

fn build_grep_notices(details: &Map, truncation: &crate::core::tools::truncate::TruncationResult) -> Vec<JsString> {
    let mut notices = Vec::new();
    if let Some(limit) = details
        .get("matchLimitReached")
        .filter(|value| value.as_f64().is_some_and(|limit| limit != 0.0))
    {
        notices.push(js_format!(
            "{} matches limit reached",
            js_json::stringify_lossless(limit)
        ));
    }
    if details.get("linesTruncated").and_then(Value::as_bool) == Some(true) {
        notices.push("long lines truncated".into());
    }
    if truncation.truncated {
        notices.push(js_format!(
            "{} limit reached",
            pi_agent_core::harness::utils::format_size(DEFAULT_MAX_BYTES as f64)
        ));
    }
    notices
}

async fn execute_sandboxed_grep(
    params: &SandboxParams,
    input: &Value,
    signal: Option<AbortSignal>,
) -> Result<AgentToolResult, JsString> {
    let search_path = input
        .get("path")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_else(|| ".".into());
    let mut payload = Map::new();
    for key in ["context", "glob", "ignoreCase"] {
        if let Some(value) = input.get(key).filter(|value| !value.is_null()) {
            payload.insert(key.into(), value.clone());
        }
    }
    payload.insert(
        "limit".into(),
        input
            .get("limit")
            .cloned()
            .filter(|limit| !limit.is_null())
            .unwrap_or_else(|| json!(DEFAULT_GREP_LIMIT)),
    );
    if let Some(value) = input.get("literal").filter(|value| !value.is_null()) {
        payload.insert("literal".into(), value.clone());
    }
    payload.insert("path".into(), Value::String(search_path.to_owned()));
    if let Some(value) = input.get("pattern").filter(|value| !value.is_null()) {
        payload.insert("pattern".into(), value.clone());
    }
    let result = run_sandbox_operation(params, "grep", payload, signal).await?;
    let text = result
        .get("text")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_default();
    let truncation = crate::core::tools::truncate::truncate_head(
        text,
        pi_agent_core::harness::utils::TruncationOptions {
            max_lines: usize::MAX as f64,
            max_bytes: DEFAULT_MAX_BYTES as f64,
        },
    );
    let mut details = result
        .get("details")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if truncation.truncated {
        details.insert(
            "truncation".into(),
            js_json::to_value(&truncation).unwrap_or(Value::Null),
        );
    }
    let notices = build_grep_notices(&details, &truncation);
    let text = if notices.is_empty() {
        truncation.content
    } else {
        truncation.content + "\n\n[" + JsString::join(notices, ". ") + "]"
    };
    Ok(AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: (!details.is_empty()).then_some(Value::Object(details).into()),
        is_error: None,
        terminate: None,
    })
}

struct BwrapRuntime {
    bwrap_command: JsString,
    local_cwd: JsString,
}

fn create_bwrap_runtime(options: &BwrapExtensionOptions) -> Result<BwrapRuntime, HandlerError> {
    Ok(BwrapRuntime {
        bwrap_command: options.bwrap_command.clone().unwrap_or_else(default_bwrap_command),
        local_cwd: process_cwd().map_err(|error| HandlerError::new(error.message()))?,
    })
}

fn register_bwrap_session_start(pi: &ExtensionApi, runtime: &BwrapRuntime) -> Result<(), String> {
    let bwrap_command = runtime.bwrap_command.clone();
    pi.on_session_start(move |_event, ctx| {
        let bwrap_command = bwrap_command.clone();
        Box::pin(async move {
            let profile = read_sandbox_profile(&ctx);
            // `ctx.ui.theme` throws before `initTheme`; the source's runner
            // records the error, so surface it as a handler error.
            let theme = crate::modes::interactive::theme::try_current_theme()
                .map_err(|error| HandlerError::new(error.message()))?;
            let Some(profile) = profile else {
                if let Ok(ui) = ctx.ui() {
                    ui.set_status(
                        &"bwrap".into(),
                        Some(crate::core::footer_data_provider::ExtensionStatus::Text(
                            theme.fg(ThemeColor::Muted, "bwrap: full access"),
                        )),
                    )?;
                }
                return Ok(None);
            };
            if let Ok(ui) = ctx.ui() {
                match run_bwrap_availability_check(&bwrap_command) {
                    Ok(()) => ui.set_status(
                        &"bwrap".into(),
                        Some(crate::core::footer_data_provider::ExtensionStatus::Text(
                            theme.fg(ThemeColor::Accent, &js_format!("bwrap: {}", profile.as_str())),
                        )),
                    )?,
                    Err(message) => {
                        ui.set_status(
                            &"bwrap".into(),
                            Some(crate::core::footer_data_provider::ExtensionStatus::Text(
                                theme.fg(ThemeColor::Error, "bwrap unavailable"),
                            )),
                        )?;
                        ui.notify(&message, Some(NotifyType::Error))?;
                    }
                }
            }
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })
}

fn register_bwrap_tool_gate(pi: &ExtensionApi, runtime: &BwrapRuntime) -> Result<(), String> {
    let bwrap_command = runtime.bwrap_command.clone();
    pi.register_tool_gate(Arc::new(move |_event, ctx| {
        let bwrap_command = bwrap_command.clone();
        Box::pin(async move {
            if read_sandbox_profile(&ctx).is_none() {
                return Ok(None);
            }
            match run_bwrap_availability_check(&bwrap_command) {
                Ok(()) => Ok(None),
                Err(reason) => Ok(Some(crate::core::extensions::types::ToolCallEventResult {
                    block: Some(true),
                    input: None,
                    reason: Some(reason.into()),
                    terminate: None,
                })),
            }
        })
            as BoxFuture<'static, Result<Option<crate::core::extensions::types::ToolCallEventResult>, HandlerError>>
    }))
}

type CreateSandboxedTool = Arc<dyn Fn(&JsString, &SandboxParams) -> ToolDefinition + Send + Sync>;

fn register_sandboxed_tool(
    pi: &ExtensionApi,
    local_tool: ToolDefinition,
    runtime: &BwrapRuntime,
    create_local: Arc<dyn Fn(&JsString) -> ToolDefinition + Send + Sync>,
    create_sandboxed: CreateSandboxedTool,
) -> Result<(), crate::core::extensions::types::HandlerError> {
    let bwrap_command = runtime.bwrap_command.clone();
    let mut definition = local_tool;
    definition.execute = Arc::new(move |id, params, signal, on_update, ctx| {
        let create_local = Arc::clone(&create_local);
        let create_sandboxed = Arc::clone(&create_sandboxed);
        let bwrap_command = bwrap_command.clone();
        Box::pin(async move {
            let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
            let cwd = ctx.cwd()?;
            let sandbox = read_sandbox_params(&ctx, &bwrap_command)?;
            let tool = match sandbox {
                Some(sandbox) => create_sandboxed(&cwd, &sandbox),
                None => create_local(&cwd),
            };
            (tool.execute)(id, params, signal, on_update, Some(ctx)).await
        })
    });
    pi.register_tool(definition)
}

fn register_standard_sandboxed_tools(
    pi: &ExtensionApi,
    runtime: &BwrapRuntime,
) -> Result<(), crate::core::extensions::types::HandlerError> {
    let local_read = create_read_tool_definition(&runtime.local_cwd, ReadToolOptions::default());
    register_sandboxed_tool(
        pi,
        local_read,
        runtime,
        Arc::new(|cwd| create_read_tool_definition(cwd, ReadToolOptions::default())),
        Arc::new(|cwd, params| {
            create_read_tool_definition(
                cwd,
                ReadToolOptions {
                    operations: Some(Arc::new(SandboxReadOperations { params: params.clone() })),
                    ..Default::default()
                },
            )
        }),
    )?;
    let local_write = create_write_tool_definition(&runtime.local_cwd, WriteToolOptions::default());
    register_sandboxed_tool(
        pi,
        local_write,
        runtime,
        Arc::new(|cwd| create_write_tool_definition(cwd, WriteToolOptions::default())),
        Arc::new(|cwd, params| {
            create_write_tool_definition(
                cwd,
                WriteToolOptions {
                    operations: Some(Arc::new(SandboxWriteOperations { params: params.clone() })),
                },
            )
        }),
    )?;
    let local_edit = create_edit_tool_definition(&runtime.local_cwd, EditToolOptions::default());
    register_sandboxed_tool(
        pi,
        local_edit,
        runtime,
        Arc::new(|cwd| create_edit_tool_definition(cwd, EditToolOptions::default())),
        Arc::new(|cwd, params| {
            create_edit_tool_definition(
                cwd,
                EditToolOptions {
                    operations: Some(Arc::new(SandboxEditOperations {
                        read: SandboxReadOperations { params: params.clone() },
                        write: SandboxWriteOperations { params: params.clone() },
                    })),
                },
            )
        }),
    )?;
    let local_ls = create_ls_tool_definition(&runtime.local_cwd, LsToolOptions::default());
    register_sandboxed_tool(
        pi,
        local_ls,
        runtime,
        Arc::new(|cwd| create_ls_tool_definition(cwd, LsToolOptions::default())),
        Arc::new(|cwd, params| {
            create_ls_tool_definition(
                cwd,
                LsToolOptions {
                    operations: Some(Arc::new(SandboxLsOperations { params: params.clone() })),
                },
            )
        }),
    )?;
    let local_find = create_find_tool_definition(&runtime.local_cwd, FindToolOptions::default());
    register_sandboxed_tool(
        pi,
        local_find,
        runtime,
        Arc::new(|cwd| create_find_tool_definition(cwd, FindToolOptions::default())),
        Arc::new(|cwd, params| {
            create_find_tool_definition(
                cwd,
                FindToolOptions {
                    operations: Some(Arc::new(SandboxFindOperations { params: params.clone() })),
                },
            )
        }),
    )
}

fn register_sandboxed_bash_tool(
    pi: &ExtensionApi,
    runtime: &BwrapRuntime,
) -> Result<(), crate::core::extensions::types::HandlerError> {
    let mut local_bash = create_bash_tool_definition(&runtime.local_cwd, Default::default());
    let bwrap_command = runtime.bwrap_command.clone();
    local_bash.execute = Arc::new(move |id, params, signal, on_update, ctx| {
        let bwrap_command = bwrap_command.clone();
        Box::pin(async move {
            let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
            let cwd = ctx.cwd()?;
            let sandbox = read_sandbox_params(&ctx, &bwrap_command)?;
            let tool = match sandbox {
                Some(sandbox) => create_bash_tool_definition(
                    &cwd,
                    crate::core::tools::bash::BashToolOptions {
                        operations: Some(create_sandboxed_bash_operations(
                            sandbox.bwrap_command,
                            Some(sandbox.profile),
                            None,
                        )),
                        ..Default::default()
                    },
                ),
                None => create_bash_tool_definition(&cwd, Default::default()),
            };
            (tool.execute)(id, params, signal, on_update, Some(ctx)).await
        })
    });
    pi.register_tool(local_bash)
}

fn register_sandboxed_grep_tool(
    pi: &ExtensionApi,
    runtime: &BwrapRuntime,
) -> Result<(), crate::core::extensions::types::HandlerError> {
    let mut local_grep = create_grep_tool_definition(&runtime.local_cwd, GrepToolOptions::default());
    let bwrap_command = runtime.bwrap_command.clone();
    local_grep.execute = Arc::new(move |_id, params, signal, on_update, ctx| {
        let bwrap_command = bwrap_command.clone();
        Box::pin(async move {
            let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
            let cwd = ctx.cwd()?;
            let sandbox = read_sandbox_params(&ctx, &bwrap_command)?;
            match sandbox {
                Some(sandbox) => execute_sandboxed_grep(&sandbox, &params, signal)
                    .await
                    .map_err(HandlerError::from),
                None => {
                    let tool = create_grep_tool_definition(&cwd, GrepToolOptions::default());
                    (tool.execute)("grep".into(), params, signal, on_update, Some(ctx)).await
                }
            }
        })
    });
    pi.register_tool(local_grep)
}

fn register_user_bash_handler(pi: &ExtensionApi, runtime: &BwrapRuntime) -> Result<(), String> {
    let bwrap_command = runtime.bwrap_command.clone();
    pi.on_user_bash(move |_event: UserBashEvent, ctx: ExtensionContext| {
        let bwrap_command = bwrap_command.clone();
        Box::pin(async move {
            let profile = read_sandbox_profile(&ctx);
            Ok(Some(UserBashEventResult {
                operations: Some(create_sandboxed_bash_operations(bwrap_command, profile, None)),
                result: None,
            }))
        }) as BoxFuture<'static, Result<Option<UserBashEventResult>, HandlerError>>
    })
}

fn register_before_agent_start_handler(pi: &ExtensionApi) -> Result<(), String> {
    pi.on_before_agent_start(move |event: crate::core::extensions::types::BeforeAgentStartEvent, ctx: ExtensionContext| {
        Box::pin(async move {
            let Some(profile) = read_sandbox_profile(&ctx) else {
                return Ok(None);
            };
            let cwd = ctx.cwd()?;
            let local_line = js_format!("Current working directory: {}", cwd);
            let sandbox_line = js_format!(
                "Current working directory: {} (Linux bubblewrap sandbox, profile {}; HOME is fake, only runtime paths plus workspace are mounted, and Pyrun executes inside the sandbox without Pi bridge capabilities)",
                cwd, profile.as_str()
            );
            let system_prompt = if event.system_prompt.contains(&local_line) {
                { let mut prompt = event.system_prompt.clone(); let start = prompt.find(&local_line).expect("line found"); prompt.replace_range(start..start + local_line.len(), &sandbox_line); prompt }
            } else {
                event.system_prompt.clone() + js_format!("\n\n{}", sandbox_line)
            };
            Ok(Some(crate::core::extensions::types::BeforeAgentStartEventResult {
                message: None,
                system_prompt: Some(system_prompt),
            }))
        }) as BoxFuture<
            'static,
            Result<Option<crate::core::extensions::types::BeforeAgentStartEventResult>, HandlerError>,
        >
    })
}

fn register_bwrap_status_command(pi: &ExtensionApi, runtime: &BwrapRuntime) -> Result<(), JsString> {
    let bwrap_command = runtime.bwrap_command.clone();
    pi.register_command(
        "bwrap",
        Some("Show bubblewrap sandbox status".into()),
        None,
        Arc::new(move |_args, ctx| {
            let bwrap_command = bwrap_command.clone();
            Box::pin(async move {
                let profile = read_sandbox_profile(&ctx);
                let (mode, notify_type) = match profile {
                    Some(profile) => match run_bwrap_availability_check(&bwrap_command) {
                        Ok(()) => (js_format!("sandboxed ({})", profile.as_str()), NotifyType::Info),
                        Err(reason) => (
                            js_format!("{}: {}; tools blocked", profile.as_str(), reason),
                            NotifyType::Error,
                        ),
                    },
                    None => (JsString::from("unsandboxed/full-access"), NotifyType::Info),
                };
                if let Ok(ui) = ctx.ui() {
                    let cwd = ctx.cwd()?;
                    ui.notify(
                        &JsString::join(
                            [
                                js_format!("bwrap backend: {}", mode),
                                js_format!("cwd: {}", cwd),
                                js_format!("bwrap: {}", bwrap_command),
                            ],
                            "\n",
                        ),
                        Some(notify_type),
                    )?;
                }
                Ok(())
            }) as BoxFuture<'static, Result<(), HandlerError>>
        }),
    )
}

/// `bwrapExtension`.
pub fn install_bwrap_extension(pi: &ExtensionApi, options: BwrapExtensionOptions) -> Result<(), HandlerError> {
    let runtime = create_bwrap_runtime(&options)?;
    register_bwrap_session_start(pi, &runtime).map_err(HandlerError::from)?;
    register_bwrap_tool_gate(pi, &runtime).map_err(HandlerError::from)?;
    register_standard_sandboxed_tools(pi, &runtime).map_err(HandlerError::from)?;
    register_sandboxed_bash_tool(pi, &runtime).map_err(HandlerError::from)?;
    register_sandboxed_grep_tool(pi, &runtime).map_err(HandlerError::from)?;
    register_user_bash_handler(pi, &runtime).map_err(HandlerError::from)?;
    register_before_agent_start_handler(pi).map_err(HandlerError::from)?;
    register_bwrap_status_command(pi, &runtime).map_err(HandlerError::from)
}

/// Registered factory for `<first-party:bwrap>`.
pub fn bwrap_extension_factory(options: BwrapExtensionOptions) -> ExtensionFactory {
    Arc::new(move |pi| {
        let result = install_bwrap_extension(pi, options.clone());
        Box::pin(async move { result })
    })
}
