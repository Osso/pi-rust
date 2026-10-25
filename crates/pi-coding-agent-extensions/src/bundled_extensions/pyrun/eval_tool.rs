//! Port of `packages/coding-agent/extensions/pyrun/src/eval-tool.ts`.

use std::sync::Arc;

use pi_agent_core::AgentToolResult;
use pi_ai::request::CallbackError;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::{
    js_json as json,
    js_json::{self, JsObject as Map, JsString, JsValue as Value, encode_node_utf8},
};

use crate::core::extensions::types::{BoxFuture, ExtensionContext, HandlerError};

use super::runner::{
    CanonicalPyrunConsoleEntry, CanonicalPyrunEvalParams, CanonicalPyrunEvalResult, CanonicalPyrunProgressUpdate,
    PiRequest, PyrunPiRequestHandler, PyrunRunnerClient, PyrunRunnerError,
};

pub const STREAMED_CONSOLE_LINE_LIMIT: usize = 300;
pub const STREAMED_CONSOLE_BYTE_LIMIT: usize = 1_048_576;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PyrunEvalParams {
    pub code: JsString,
    pub session_id: Option<JsString>,
}

pub type PyrunPiRequestDispatcher = Arc<
    dyn Fn(
            PiRequest,
            ExtensionContext,
            Option<AbortSignal>,
            Option<String>,
        ) -> BoxFuture<'static, Result<Option<Value>, HandlerError>>
        + Send
        + Sync,
>;

/// `capConsoleText`.
#[cfg(test)]
mod lossless_tests {
    use super::*;
    #[test]
    fn console_keeps_raw_units_until_actual_buffer_truncation() {
        let text = JsString::from_utf16(vec![0xd800, 0x61]);
        assert_eq!(cap_console_text(&text, 4), text);
        assert_eq!(cap_console_text(&text, 3).units(), &[0x61]);
        let result = super::super::runner::CanonicalPyrunEvalResult {
            value: Some(Value::String(text.clone())),
            result_type: "value".into(),
            ..Default::default()
        };
        assert_eq!(format_result_value(&result), Some(text));
    }
}

pub fn cap_console_text(text: &JsString, byte_limit: usize) -> JsString {
    let encoded = encode_node_utf8(text);
    let bytes = encoded.as_bytes();
    if bytes.len() <= byte_limit {
        return text.to_owned();
    }
    let mut start = bytes.len() - byte_limit;
    while start < bytes.len() && (bytes[start] & 0xc0) == 0x80 {
        start += 1;
    }
    String::from_utf8_lossy(&bytes[start..]).into_owned().into()
}

/// `appendCappedConsoleText`.
pub fn append_capped_console_text(existing_text: &JsString, new_text: &JsString) -> JsString {
    let text = cap_console_text(existing_text, STREAMED_CONSOLE_BYTE_LIMIT)
        + cap_console_text(new_text, STREAMED_CONSOLE_BYTE_LIMIT);
    let has_trailing_newline = text.ends_with("\n");
    let mut lines: Vec<JsString> = text.split('\n').collect();
    if has_trailing_newline {
        lines.pop();
    }
    let start = lines.len().saturating_sub(STREAMED_CONSOLE_LINE_LIMIT);
    let capped_text = JsString::join(lines[start..].iter(), "\n");
    cap_console_text(
        &if has_trailing_newline {
            capped_text + "\n"
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
pub fn format_result_value(result: &CanonicalPyrunEvalResult) -> Option<JsString> {
    if result.result_type == "needs_approval" {
        let summary = result
            .approval
            .as_ref()
            .map(|approval| &approval.summary)
            .cloned()
            .unwrap_or_else(|| "unknown Pyrun operation".into());
        return Some(JsString::from("needs approval: ") + summary);
    }
    let value = result.value.as_ref()?;
    let command_exit_code = command_result_exit_code(value);
    if command_exit_code == Some(0.0) {
        return None;
    }
    if let Some(exit_code) = command_exit_code {
        return Some(format!("exit code {}", pi_js_compat::number::js_number_to_string(exit_code)).into());
    }
    if value.is_null() {
        return None;
    }
    if let Some(text) = value.as_string() {
        return if text.trim().is_empty() {
            None
        } else {
            Some(text.to_owned())
        };
    }
    Some(js_json::stringify_lossless(value).into())
}

/// `formatConsoleEntry`.
pub fn format_console_entry(entry: &CanonicalPyrunConsoleEntry) -> JsString {
    match entry {
        CanonicalPyrunConsoleEntry::Text(text) => text.clone(),
        CanonicalPyrunConsoleEntry::Leveled { level, message } => JsString::from(level) + ": " + message,
    }
}

fn count_console_lines(text: &JsString) -> usize {
    let content = text.strip_suffix("\n").unwrap_or_else(|| text.clone());
    content.split('\n').count()
}

fn cap_console_lines(text: &JsString, line_limit: usize) -> JsString {
    let has_trailing_newline = text.ends_with("\n");
    let mut lines: Vec<JsString> = text.split('\n').collect();
    if has_trailing_newline {
        lines.pop();
    }
    if lines.len() <= line_limit {
        return text.to_owned();
    }
    let start = lines.len() - line_limit;
    let capped_text = JsString::join(lines[start..].iter(), "\n");
    if has_trailing_newline {
        capped_text + "\n"
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
        remaining_bytes = remaining_bytes.saturating_sub(encode_node_utf8(&text).len());
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
pub fn format_tool_text(params: &PyrunEvalParams, result: &CanonicalPyrunEvalResult) -> JsString {
    let mut lines = vec![params.code.clone(), JsString::default()];
    for entry in result.console.as_deref().unwrap_or_default() {
        lines.push(format_console_entry(entry));
    }
    if let Some(error) = &result.error {
        lines.push(JsString::from("Error: ") + error);
    } else if let Some(formatted) = format_result_value(result) {
        lines.push(formatted);
    }
    JsString::join(lines, "\n")
}

/// `formatProgressText`.
pub fn format_progress_text(update: &CanonicalPyrunProgressUpdate) -> JsString {
    if update.message_type() == "pi_request"
        && let Some(method) = update.method()
    {
        return pi_js_compat::js_format!("Pi request: {}", method);
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
        return status.into();
    }
    if let Some(value) = update.value() {
        return match value {
            Value::String(text) => text.clone(),
            other => js_json::stringify_lossless(other).into(),
        };
    }
    update.message_type()
}

pub struct PyrunEvalContext {
    pub footer_data: Option<Arc<crate::core::footer_data_provider::FooterDataProvider>>,
    pub context_usage: Option<crate::core::extensions::types::ContextUsage>,
    pub model: Option<pi_ai::types::Model>,
    pub cwd: JsString,
    pub session_name: Option<JsString>,
}

/// `createPiCapabilitySnapshot`.
pub fn create_pi_capability_snapshot(ctx: &PyrunEvalContext) -> Result<Value, CallbackError> {
    let mut footer = Map::new();
    footer.insert(
        "availableProviderCount".into(),
        json!(
            ctx.footer_data
                .as_ref()
                .map_or(0.0, |footer| footer.get_available_provider_count())
        ),
    );
    footer.insert(
        "branch".into(),
        ctx.footer_data
            .as_ref()
            .and_then(|footer| footer.get_git_branch())
            .map_or(Value::Null, |value| Value::String(value.into())),
    );
    if let Some(context_usage) = &ctx.context_usage {
        footer.insert(
            "contextUsage".into(),
            js_json::to_value(context_usage).unwrap_or(Value::Null),
        );
    }
    footer.insert("cwd".into(), Value::String(ctx.cwd.clone().into()));
    let mut extension_statuses = Map::new();
    if let Some(footer_data) = &ctx.footer_data {
        for (key, value) in footer_data.get_extension_statuses()? {
            extension_statuses.insert(key, Value::String(value));
        }
    }
    footer.insert("extensionStatuses".into(), Value::Object(extension_statuses));
    footer.insert(
        "model".into(),
        ctx.model
            .as_ref()
            .map_or(Value::Null, |model| Value::String(model.id.clone().into())),
    );
    footer.insert(
        "sessionName".into(),
        ctx.session_name
            .clone()
            .map_or(Value::Null, |value| Value::String(value.into())),
    );
    let mut snapshot = Map::new();
    snapshot.insert("footer".into(), Value::Object(footer));
    Ok(Value::Object(snapshot))
}

pub fn eval_context(ctx: &ExtensionContext) -> Result<PyrunEvalContext, HandlerError> {
    let session = ctx.session_manager()?;
    let (cwd, session_name) = {
        let manager = session;
        (
            manager.get_cwd().to_owned(),
            manager.get_session_name().map(JsString::from),
        )
    };
    Ok(PyrunEvalContext {
        footer_data: ctx.footer_data()?,
        context_usage: ctx.get_context_usage()?,
        model: ctx.model()?,
        cwd,
        session_name,
    })
}

/// `createCanonicalPyrunEvalParams`.
pub fn create_canonical_pyrun_eval_params(
    params: &PyrunEvalParams,
    ctx: &PyrunEvalContext,
    pi_bridge_enabled: bool,
) -> Result<CanonicalPyrunEvalParams, CallbackError> {
    Ok(CanonicalPyrunEvalParams {
        code: params.code.clone(),
        pi: if pi_bridge_enabled {
            Some(create_pi_capability_snapshot(ctx)?)
        } else {
            None
        },
        pi_bridge: Some(pi_bridge_enabled),
        session_id: params.session_id.clone(),
        stream_console: Some(true),
    })
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
        details: Some(js_json::to_value(&bounded_result).unwrap_or(Value::Null).into()),
        is_error: Some(is_error),
        terminate: None,
    }
}

pub use pi_agent_core::agent::AgentToolUpdateCallback as PyrunProgressUpdateCallback;

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
) -> Arc<dyn Fn(CanonicalPyrunProgressUpdate) -> Result<(), HandlerError> + Send + Sync> {
    let streamed_console_text = Arc::new(std::sync::Mutex::new(JsString::default()));
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
                details: Some(bounded_update.raw.into()),
                is_error: None,
                terminate: None,
            })
            .map_err(|error| HandlerError::new(error.message))?;
        }
        Ok(())
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
            Option<AbortSignal>,
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
                            Err(HandlerError::new(pi_js_compat::js_format!(
                                "Pi capability is unavailable: {}",
                                method
                            )))
                        }) as BoxFuture<'static, Result<Option<Value>, HandlerError>>;
                    };
                    dispatch(request, ctx.clone(), signal.clone(), active_tool_call_id.clone())
                }))
            } else {
                None
            };
            let eval_context = eval_context(&ctx)?;
            let canonical_params = create_canonical_pyrun_eval_params(&params, &eval_context, enable_pi_bridge)?;
            let report_progress = create_pyrun_progress_reporter(on_update);
            let result = runner
                .evaluate(
                    canonical_params,
                    Some(Arc::new(move |update| {
                        report_progress(update).map_err(|error| PyrunRunnerError::new(error.message))
                    })),
                    signal,
                    on_pi_request,
                )
                .await
                .map_err(|error: PyrunRunnerError| HandlerError::new(error.message().clone()))?;
            Ok(format_canonical_pyrun_eval_result(&params, result))
        })
    })
}
