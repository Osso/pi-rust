use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use pi_ai::request::BoxFuture;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use pi_js_compat::number::js_number_to_string;
use serde_json::{Value, json};

use crate::agent::{AgentTool, AgentToolResult, AgentToolUpdateCallback, ToolError};
use crate::harness::tools::tool_context::ExecutionToolContext;
use crate::harness::types::ExecutionErrorCode;
use crate::harness::utils::{
    DEFAULT_MAX_BYTES, ShellCaptureOptions, ShellCaptureProgress, ShellCaptureResult, execute_shell_with_capture,
    format_size,
};
use tokio_util::sync::CancellationToken;

const MAX_TIMEOUT_SECONDS: f64 = 2_147_483_647.0 / 1000.0;
const BASH_UPDATE_THROTTLE_MS: u128 = 100;
const DESCRIPTION: &str = "Execute a bash command in the current working directory. Returns stdout and stderr. Output is truncated to last 2000 lines or 10KB (whichever is hit first). If truncated, full output is saved to a temp file. Optionally provide a timeout in seconds.";

/// Command, working directory, and environment prepared before shell execution.
#[derive(Debug, Clone)]
pub struct BashExecution {
    pub command: JsString,
    pub cwd: JsString,
    pub env: HashMap<String, String>,
    pub inherit_env: bool,
}

/// Hook that may adjust a [`BashExecution`] before it runs.
pub trait BashPrepare: Send + Sync {
    fn prepare<'a>(
        &'a self,
        execution: &'a mut BashExecution,
        context: &'a ExecutionToolContext,
        signal: Option<&'a CancellationToken>,
    ) -> BoxFuture<'a, Result<(), String>>;
}

/// Options for [`BashTool`].
#[derive(Clone, Default)]
pub struct BashToolOptions {
    pub command_prefix: Option<String>,
    pub prepare: Option<Arc<dyn BashPrepare>>,
}

/// Executes bash commands through the execution environment.
pub struct BashTool {
    context: Arc<ExecutionToolContext>,
    parameters: Value,
    options: BashToolOptions,
}

impl BashTool {
    pub fn new(context: Arc<ExecutionToolContext>) -> Self {
        Self::with_options(context, BashToolOptions::default())
    }

    pub fn with_options(context: Arc<ExecutionToolContext>, options: BashToolOptions) -> Self {
        Self {
            context,
            parameters: bash_schema(),
            options,
        }
    }
}

fn bash_schema() -> Value {
    json!({
        "type": "object",
        "required": ["command"],
        "properties": {
            "command": {"type": "string", "description": "Bash command to execute"},
            "timeout": {"type": "number", "description": "Timeout in seconds (optional, no default timeout)"}
        }
    })
}

impl AgentTool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn label(&self) -> &str {
        "bash"
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        signal: Option<CancellationToken>,
        on_update: AgentToolUpdateCallback,
        _execution: Option<crate::harness::types::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        let context = Arc::clone(&self.context);
        let options = self.options.clone();
        Box::pin(async move { execute_bash(context, options, params, signal, on_update).await })
    }
}

struct UpdateThrottle {
    on_update: AgentToolUpdateCallback,
    latest: Mutex<Option<ShellCaptureProgress>>,
    last_update_at: Mutex<Instant>,
}

impl UpdateThrottle {
    fn new(on_update: AgentToolUpdateCallback) -> Self {
        Self {
            on_update,
            latest: Mutex::new(None),
            last_update_at: Mutex::new(
                Instant::now()
                    .checked_sub(std::time::Duration::from_millis(BASH_UPDATE_THROTTLE_MS as u64))
                    .unwrap_or_else(Instant::now),
            ),
        }
    }

    fn emit_initial(&self) {
        (self.on_update)(AgentToolResult {
            content: Vec::new(),
            details: None,
            is_error: None,
            terminate: None,
        });
    }

    fn record(&self, progress: &ShellCaptureProgress) {
        *self.latest.lock().expect("latest progress lock") = Some(progress.clone());
        let mut last_update_at = self.last_update_at.lock().expect("update throttle lock");
        if last_update_at.elapsed().as_millis() < BASH_UPDATE_THROTTLE_MS {
            return;
        }
        *last_update_at = Instant::now();
        drop(last_update_at);
        self.emit_latest();
    }

    fn emit_latest(&self) {
        let Some(progress) = self.latest.lock().expect("latest progress lock").clone() else {
            return;
        };
        (self.on_update)(progress_result(&progress));
    }
}

fn progress_result(progress: &ShellCaptureProgress) -> AgentToolResult {
    let mut details = JsObject::new();
    if progress.truncation.truncated {
        details.insert(
            "truncation".into(),
            pi_js_compat::js_json::to_value(&progress.truncation).expect("truncation metadata must serialize"),
        );
    }
    if let Some(path) = &progress.full_output_path {
        details.insert("fullOutputPath".into(), JsValue::String(path.clone()));
    }
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(progress.output.clone()))],
        details: Some(JsValue::Object(details)),
        is_error: None,
        terminate: None,
    }
}

async fn execute_bash(
    context: Arc<ExecutionToolContext>,
    options: BashToolOptions,
    params: JsValue,
    signal: Option<CancellationToken>,
    on_update: AgentToolUpdateCallback,
) -> Result<AgentToolResult, ToolError> {
    let (command, timeout) = parse_bash_input(&params)?;
    let execution = prepare_execution(&context, &options, command, signal.as_ref()).await?;
    let throttle = Arc::new(UpdateThrottle::new(on_update));
    throttle.emit_initial();

    let capture = capture_shell(&context, &execution, timeout, signal, Arc::clone(&throttle)).await?;
    (throttle.on_update)(capture_update_result(&capture));

    let mut output_text = JsString::from(capture.output.clone());
    let details = append_truncation(&mut output_text, &capture);
    if let Some(error) = capture_failure(&capture, timeout, &output_text) {
        return Err(error);
    }
    Ok(AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(if output_text.is_empty() {
            "(no output)".into()
        } else {
            output_text
        }))],
        details,
        is_error: None,
        terminate: None,
    })
}

fn parse_bash_input(params: &JsValue) -> Result<(JsString, Option<f64>), ToolError> {
    let command = params
        .get("command")
        .and_then(JsValue::as_string)
        .ok_or_else(|| ToolError::new("bash tool requires a string command"))?
        .clone();
    let timeout = params.get("timeout").and_then(JsValue::as_f64);
    validate_timeout(timeout)?;
    Ok((command, timeout))
}

async fn prepare_execution(
    context: &ExecutionToolContext,
    options: &BashToolOptions,
    command: JsString,
    signal: Option<&CancellationToken>,
) -> Result<BashExecution, ToolError> {
    let mut execution = BashExecution {
        command: match &options.command_prefix {
            Some(prefix) => JsString::from(prefix) + "\n" + command,
            None => command,
        },
        cwd: context.env.cwd().to_owned(),
        env: HashMap::new(),
        inherit_env: true,
    };
    if let Some(prepare) = &options.prepare {
        prepare
            .prepare(&mut execution, context, signal)
            .await
            .map_err(ToolError::new)?;
    }
    Ok(execution)
}

async fn capture_shell(
    context: &ExecutionToolContext,
    execution: &BashExecution,
    timeout: Option<f64>,
    signal: Option<CancellationToken>,
    throttle: Arc<UpdateThrottle>,
) -> Result<ShellCaptureResult, ToolError> {
    execute_shell_with_capture(
        &context.env,
        &execution.command,
        ShellCaptureOptions {
            cwd: Some(execution.cwd.clone()),
            env: Some(execution.env.clone()),
            inherit_env: Some(execution.inherit_env),
            timeout,
            abort: signal,
            on_chunk: Some(Arc::new(move |_chunk: &str, progress: &ShellCaptureProgress| {
                throttle.record(progress);
            })),
            return_execution_errors: true,
        },
    )
    .await
    .map_err(|error| ToolError::new(error.message))
}

fn append_truncation(output_text: &mut JsString, capture: &ShellCaptureResult) -> Option<JsValue> {
    if !capture.truncation.truncated {
        return None;
    }
    output_text.push_str(truncation_notice(capture));
    let mut details = JsObject::new();
    details.insert(
        "truncation".into(),
        pi_js_compat::js_json::to_value(&capture.truncation).expect("truncation metadata must serialize"),
    );
    if let Some(path) = &capture.full_output_path {
        details.insert("fullOutputPath".into(), JsValue::String(path.clone()));
    }
    Some(JsValue::Object(details))
}

fn truncation_notice(capture: &ShellCaptureResult) -> JsString {
    let start_line = capture.truncation.total_lines - capture.truncation.output_lines + 1;
    let end_line = capture.truncation.total_lines;
    let full_output_path = capture.full_output_path.clone().unwrap_or_default();
    if capture.truncation.last_line_partial {
        return js_format!(
            "\n\n[Showing last {} of line {} (line is {}). Full output: {}]",
            format_size(capture.truncation.output_bytes),
            end_line.to_string(),
            format_size(capture.last_line_bytes),
            full_output_path
        );
    }
    if capture.truncation.truncated_by == Some(crate::harness::utils::TruncatedBy::Lines) {
        return js_format!(
            "\n\n[Showing lines {}-{} of {}. Full output: {}]",
            start_line.to_string(),
            end_line.to_string(),
            capture.truncation.total_lines.to_string(),
            full_output_path
        );
    }
    js_format!(
        "\n\n[Showing lines {}-{} of {} ({} limit). Full output: {}]",
        start_line.to_string(),
        end_line.to_string(),
        capture.truncation.total_lines.to_string(),
        format_size(DEFAULT_MAX_BYTES),
        full_output_path
    )
}

fn capture_failure(capture: &ShellCaptureResult, timeout: Option<f64>, output_text: &JsString) -> Option<ToolError> {
    if capture.cancelled {
        return Some(ToolError::new(append_status(output_text, "Command aborted")));
    }
    if let Some(error) = &capture.execution_error {
        if error.code == ExecutionErrorCode::Timeout {
            let timeout = timeout.map(js_number_to_string).unwrap_or_default();
            return Some(ToolError::new(append_status(
                output_text,
                &format!("Command timed out after {timeout} seconds"),
            )));
        }
        return Some(ToolError::new(error.message.clone()));
    }
    match capture.exit_code {
        Some(exit_code) if exit_code != 0 => Some(ToolError::new(append_status(
            output_text,
            &format!("Command exited with code {exit_code}"),
        ))),
        _ => None,
    }
}

fn append_status(output_text: &JsString, status: &str) -> JsString {
    if output_text.is_empty() {
        status.into()
    } else {
        js_format!("{}\n\n{}", output_text, status)
    }
}

fn validate_timeout(timeout: Option<f64>) -> Result<(), ToolError> {
    let Some(timeout) = timeout else {
        return Ok(());
    };
    if !timeout.is_finite() || timeout <= 0.0 {
        return Err(ToolError::new("Invalid timeout: must be a finite number of seconds"));
    }
    if timeout > MAX_TIMEOUT_SECONDS {
        return Err(ToolError::new(format!(
            "Invalid timeout: maximum is {} seconds",
            js_number_to_string(MAX_TIMEOUT_SECONDS)
        )));
    }
    Ok(())
}

fn capture_update_result(capture: &ShellCaptureResult) -> AgentToolResult {
    let mut details = JsObject::new();
    if capture.truncation.truncated {
        details.insert(
            "truncation".into(),
            pi_js_compat::js_json::to_value(&capture.truncation).expect("truncation metadata must serialize"),
        );
    }
    if let Some(path) = &capture.full_output_path {
        details.insert("fullOutputPath".into(), JsValue::String(path.clone()));
    }
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(capture.output.clone()))],
        details: Some(JsValue::Object(details)),
        is_error: None,
        terminate: None,
    }
}
