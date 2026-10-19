//! Port of `packages/coding-agent/extensions/pyrun/src/eval-tool.ts`.

use std::sync::Arc;

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use crate::core::extensions::types::{BoxFuture, ExtensionContext, HandlerError};
use crate::core::session_manager::lock_session_manager;

use super::runner::{
    CanonicalPyrunConsoleEntry, CanonicalPyrunEvalParams, CanonicalPyrunEvalResult, CanonicalPyrunProgressUpdate,
    PiRequest, PyrunPiRequestHandler, PyrunRunnerClient, PyrunRunnerError,
};

pub const STREAMED_CONSOLE_LINE_LIMIT: usize = 300;
pub const STREAMED_CONSOLE_BYTE_LIMIT: usize = 1_048_576;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PyrunEvalParams {
    pub code: String,
    pub session_id: Option<String>,
}

pub type PyrunPiRequestDispatcher = Arc<
    dyn Fn(
            PiRequest,
            ExtensionContext,
            Option<CancellationToken>,
            Option<String>,
        ) -> BoxFuture<'static, Result<Option<Value>, HandlerError>>
        + Send
        + Sync,
>;

/// `capConsoleText`.
pub fn cap_console_text(text: &str, byte_limit: usize) -> String {
    let bytes = text.as_bytes();
    if bytes.len() <= byte_limit {
        return text.to_owned();
    }
    let mut start = bytes.len() - byte_limit;
    while start < bytes.len() && (bytes[start] & 0xc0) == 0x80 {
        start += 1;
    }
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

/// `appendCappedConsoleText`.
pub fn append_capped_console_text(existing_text: &str, new_text: &str) -> String {
    let text = format!(
        "{}{}",
        cap_console_text(existing_text, STREAMED_CONSOLE_BYTE_LIMIT),
        cap_console_text(new_text, STREAMED_CONSOLE_BYTE_LIMIT)
    );
    let has_trailing_newline = text.ends_with('\n');
    let mut lines: Vec<&str> = text.split('\n').collect();
    if has_trailing_newline {
        lines.pop();
    }
    let start = lines.len().saturating_sub(STREAMED_CONSOLE_LINE_LIMIT);
    let capped_text = lines[start..].join("\n");
    cap_console_text(
        &if has_trailing_newline {
            format!("{capped_text}\n")
        } else {
            capped_text
        },
        STREAMED_CONSOLE_BYTE_LIMIT,
    )
}

fn command_result_exit_code(value: &Value) -> Option<f64> {
    let object = value.as_object()?;
    let has_command_output =
        object.get("stdout").is_some_and(Value::is_string) && object.get("stderr").is_some_and(Value::is_string);
    if !has_command_output {
        return None;
    }
    object.get("exit_code").and_then(Value::as_f64)
}

/// `formatResultValue`.
pub fn format_result_value(result: &CanonicalPyrunEvalResult) -> Option<String> {
    if result.result_type == "needs_approval" {
        let summary = result
            .approval
            .as_ref()
            .map(|approval| approval.summary.as_str())
            .unwrap_or("unknown Pyrun operation");
        return Some(format!("needs approval: {summary}"));
    }
    let value = result.value.as_ref()?;
    let command_exit_code = command_result_exit_code(value);
    if command_exit_code == Some(0.0) {
        return None;
    }
    if let Some(exit_code) = command_exit_code {
        return Some(format!("exit code {}", format_js_number(exit_code)));
    }
    if value.is_null() {
        return None;
    }
    if let Some(text) = value.as_str() {
        return if pi_js_compat::string::js_trim(text).is_empty() {
            None
        } else {
            Some(text.to_owned())
        };
    }
    Some(serde_json::to_string(value).unwrap_or_default())
}

fn format_js_number(value: f64) -> String {
    pi_js_compat::number::js_number_to_string(value)
}

/// `formatConsoleEntry`.
pub fn format_console_entry(entry: &CanonicalPyrunConsoleEntry) -> String {
    match entry {
        CanonicalPyrunConsoleEntry::Text(text) => text.clone(),
        CanonicalPyrunConsoleEntry::Leveled { level, message } => format!("{level}: {message}"),
    }
}

fn count_console_lines(text: &str) -> usize {
    let content = text.strip_suffix('\n').unwrap_or(text);
    content.split('\n').count()
}

fn cap_console_lines(text: &str, line_limit: usize) -> String {
    let has_trailing_newline = text.ends_with('\n');
    let mut lines: Vec<&str> = text.split('\n').collect();
    if has_trailing_newline {
        lines.pop();
    }
    if lines.len() <= line_limit {
        return text.to_owned();
    }
    let start = lines.len() - line_limit;
    let capped_text = lines[start..].join("\n");
    if has_trailing_newline {
        format!("{capped_text}\n")
    } else {
        capped_text
    }
}

/// `capConsoleHistory`.
pub fn cap_console_history(entries: &[CanonicalPyrunConsoleEntry]) -> Vec<CanonicalPyrunConsoleEntry> {
    let mut remaining_bytes = STREAMED_CONSOLE_BYTE_LIMIT;
    let mut remaining_lines = STREAMED_CONSOLE_LINE_LIMIT;
    let mut bounded = Vec::new();
    for entry in entries.iter().rev() {
        if remaining_lines == 0 || remaining_bytes == 0 {
            break;
        }
        let entry_text = match entry {
            CanonicalPyrunConsoleEntry::Text(text) => text.clone(),
            CanonicalPyrunConsoleEntry::Leveled { message, .. } => message.clone(),
        };
        let text = cap_console_lines(&cap_console_text(&entry_text, remaining_bytes), remaining_lines);
        remaining_bytes = remaining_bytes.saturating_sub(text.len());
        remaining_lines = remaining_lines.saturating_sub(count_console_lines(&text));
        bounded.push(match entry {
            CanonicalPyrunConsoleEntry::Text(_) => CanonicalPyrunConsoleEntry::Text(text),
            CanonicalPyrunConsoleEntry::Leveled { level, .. } => CanonicalPyrunConsoleEntry::Leveled {
                level: level.clone(),
                message: text,
            },
        });
    }
    bounded.reverse();
    bounded
}

/// `boundConsoleResult`.
pub fn bound_console_result(result: CanonicalPyrunEvalResult) -> CanonicalPyrunEvalResult {
    match result.console {
        Some(console) => CanonicalPyrunEvalResult {
            console: Some(cap_console_history(&console)),
            ..result
        },
        None => result,
    }
}

/// `formatToolText`.
pub fn format_tool_text(params: &PyrunEvalParams, result: &CanonicalPyrunEvalResult) -> String {
    let mut lines = vec![params.code.clone(), String::new()];
    for entry in result.console.as_deref().unwrap_or_default() {
        lines.push(format_console_entry(entry));
    }
    if let Some(error) = &result.error {
        lines.push(format!("Error: {error}"));
    } else if let Some(formatted) = format_result_value(result) {
        lines.push(formatted);
    }
    lines.join("\n")
}

/// `formatProgressText`.
pub fn format_progress_text(update: &CanonicalPyrunProgressUpdate) -> String {
    if update.message_type() == "pi_request"
        && let Some(method) = update.method()
    {
        return format!("Pi request: {method}");
    }
    if let Some(message) = update.message() {
        return message.to_owned();
    }
    if let Some(text) = update.text() {
        return text.to_owned();
    }
    if let Some(output) = update.output() {
        return output.to_owned();
    }
    if let Some(status) = update.status() {
        return status.to_owned();
    }
    if let Some(value) = update.value() {
        return match value {
            Value::String(text) => text.clone(),
            other => serde_json::to_string(other).unwrap_or_default(),
        };
    }
    update.message_type().to_owned()
}

pub struct PyrunEvalContext {
    pub footer_data: Option<Arc<crate::core::footer_data_provider::FooterDataProvider>>,
    pub context_usage: Option<crate::core::extensions::types::ContextUsage>,
    pub model: Option<pi_ai::types::Model>,
    pub cwd: String,
    pub session_name: Option<String>,
}

/// `createPiCapabilitySnapshot`.
pub fn create_pi_capability_snapshot(ctx: &PyrunEvalContext) -> Value {
    let mut extension_statuses = Map::new();
    if let Some(footer_data) = &ctx.footer_data {
        for (key, value) in footer_data.get_extension_statuses() {
            extension_statuses.insert(key, Value::String(value));
        }
    }
    let mut footer = Map::new();
    footer.insert(
        "availableProviderCount".to_owned(),
        json!(
            ctx.footer_data
                .as_ref()
                .map_or(0, |footer| footer.get_available_provider_count())
        ),
    );
    footer.insert(
        "branch".to_owned(),
        ctx.footer_data
            .as_ref()
            .and_then(|footer| footer.get_git_branch())
            .map_or(Value::Null, Value::String),
    );
    if let Some(context_usage) = &ctx.context_usage {
        footer.insert(
            "contextUsage".to_owned(),
            serde_json::to_value(context_usage).unwrap_or(Value::Null),
        );
    }
    footer.insert("cwd".to_owned(), Value::String(ctx.cwd.clone()));
    footer.insert("extensionStatuses".to_owned(), Value::Object(extension_statuses));
    footer.insert(
        "model".to_owned(),
        ctx.model
            .as_ref()
            .map_or(Value::Null, |model| Value::String(model.id.clone())),
    );
    footer.insert(
        "sessionName".to_owned(),
        ctx.session_name.clone().map_or(Value::Null, Value::String),
    );
    let mut snapshot = Map::new();
    snapshot.insert("footer".to_owned(), Value::Object(footer));
    Value::Object(snapshot)
}

pub fn eval_context(ctx: &ExtensionContext) -> Result<PyrunEvalContext, HandlerError> {
    let session = ctx
        .session_manager()
        .map_err(|error| HandlerError::new(error.to_string()))?;
    let (cwd, session_name) = {
        let manager = lock_session_manager(&session);
        (manager.get_cwd().to_owned(), manager.get_session_name())
    };
    Ok(PyrunEvalContext {
        footer_data: ctx
            .footer_data()
            .map_err(|error| HandlerError::new(error.to_string()))?,
        context_usage: ctx
            .get_context_usage()
            .map_err(|error| HandlerError::new(error.to_string()))?,
        model: ctx.model().map_err(|error| HandlerError::new(error.to_string()))?,
        cwd,
        session_name,
    })
}

/// `createCanonicalPyrunEvalParams`.
pub fn create_canonical_pyrun_eval_params(
    params: &PyrunEvalParams,
    ctx: &PyrunEvalContext,
    pi_bridge_enabled: bool,
) -> CanonicalPyrunEvalParams {
    CanonicalPyrunEvalParams {
        code: params.code.clone(),
        pi: if pi_bridge_enabled {
            Some(create_pi_capability_snapshot(ctx))
        } else {
            None
        },
        pi_bridge: Some(pi_bridge_enabled),
        session_id: params.session_id.clone(),
        stream_console: Some(true),
    }
}

/// `formatCanonicalPyrunEvalResult`.
pub fn format_canonical_pyrun_eval_result(
    params: &PyrunEvalParams,
    result: CanonicalPyrunEvalResult,
) -> AgentToolResult {
    let bounded_result = bound_console_result(result);
    let text = format_tool_text(params, &bounded_result);
    let is_error = bounded_result.error.is_some();
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(serde_json::to_value(&bounded_result).unwrap_or(Value::Null)),
        usage: None,
        added_tool_names: None,
        is_error: Some(is_error),
        terminate: None,
    }
}

pub type PyrunProgressUpdateCallback = Arc<dyn Fn(AgentToolResult) + Send + Sync>;

fn bound_progress_update(update: &CanonicalPyrunProgressUpdate) -> CanonicalPyrunProgressUpdate {
    if update.message_type() != "console" {
        return update.clone();
    }
    match update.text() {
        Some(text) => update.with_text(cap_console_text(text, STREAMED_CONSOLE_BYTE_LIMIT)),
        None => update.clone(),
    }
}

/// `createPyrunProgressReporter`.
pub fn create_pyrun_progress_reporter(
    on_update: Option<PyrunProgressUpdateCallback>,
) -> Arc<dyn Fn(CanonicalPyrunProgressUpdate) + Send + Sync> {
    let streamed_console_text = Arc::new(std::sync::Mutex::new(String::new()));
    Arc::new(move |update| {
        let bounded_update = bound_progress_update(&update);
        let formatted_progress_text = format_progress_text(&bounded_update);
        let progress_text = if bounded_update.message_type() == "console" {
            let mut streamed = streamed_console_text
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let progress_text = append_capped_console_text(&streamed, &formatted_progress_text);
            *streamed = progress_text.clone();
            progress_text
        } else {
            formatted_progress_text
        };
        if let Some(on_update) = &on_update {
            on_update(AgentToolResult {
                content: vec![TextOrImageContent::Text(TextContent::new(progress_text))],
                details: Some(bounded_update.raw),
                usage: None,
                added_tool_names: None,
                is_error: None,
                terminate: None,
            });
        }
    })
}

#[derive(Default)]
pub struct PyrunEvalExecutorOptions {
    pub enable_pi_bridge: Option<bool>,
}

pub type PyrunEvaluator = Arc<
    dyn Fn(
            PyrunEvalParams,
            ExtensionContext,
            Option<PyrunProgressUpdateCallback>,
            Option<CancellationToken>,
            Option<String>,
        ) -> BoxFuture<'static, Result<AgentToolResult, HandlerError>>
        + Send
        + Sync,
>;

/// `createPyrunEvalExecutor`.
pub fn create_pyrun_eval_executor(
    runner: PyrunRunnerClient,
    dispatch_pi_request: Option<PyrunPiRequestDispatcher>,
    options: PyrunEvalExecutorOptions,
) -> PyrunEvaluator {
    Arc::new(move |params, ctx, on_update, signal, active_tool_call_id| {
        let runner = runner.clone();
        let dispatch_pi_request = dispatch_pi_request.clone();
        let enable_pi_bridge = options.enable_pi_bridge.unwrap_or(true);
        Box::pin(async move {
            let on_pi_request: Option<PyrunPiRequestHandler> = if enable_pi_bridge {
                let dispatch_pi_request = dispatch_pi_request.clone();
                let ctx = ctx.clone();
                let signal = signal.clone();
                let active_tool_call_id = active_tool_call_id.clone();
                Some(Arc::new(move |request: PiRequest| {
                    let Some(dispatch) = dispatch_pi_request.clone() else {
                        let method = request.method.clone();
                        return Box::pin(async move {
                            Err(HandlerError::new(format!("Pi capability is unavailable: {method}")))
                        }) as BoxFuture<'static, Result<Option<Value>, HandlerError>>;
                    };
                    dispatch(request, ctx.clone(), signal.clone(), active_tool_call_id.clone())
                }))
            } else {
                None
            };
            let eval_context = eval_context(&ctx)?;
            let canonical_params = create_canonical_pyrun_eval_params(&params, &eval_context, enable_pi_bridge);
            let result = runner
                .evaluate(
                    canonical_params,
                    Some(create_pyrun_progress_reporter(on_update)),
                    signal,
                    on_pi_request,
                )
                .await
                .map_err(|error: PyrunRunnerError| HandlerError::new(error.to_string()))?;
            Ok(format_canonical_pyrun_eval_result(&params, result))
        })
    })
}
