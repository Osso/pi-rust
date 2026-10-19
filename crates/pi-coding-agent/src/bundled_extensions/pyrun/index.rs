//! Port of `packages/coding-agent/extensions/pyrun/src/index.ts` (non-detached
//! paths plus the shared dispatcher and tool definition).

use std::path::Path;
use std::sync::{Arc, Mutex};

use pi_agent_core::AgentToolResult;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::types::{TextOrImageContent, ToolCallExecution};
use pi_ai::utils::uuid::uuidv7;
use pi_tui::components::text::Text;
use pi_tui::tui::ComponentRef;
use serde_json::{Map, Value, json};

use crate::bundled_extensions::bwrap::backend::{
    BwrapRunnerCommandOptions, create_bwrap_runner_command, create_bwrap_runner_environment,
    resolve_bwrap_sandbox_profile,
};
use crate::core::extensions::loader::ExtensionFactory;
use crate::core::extensions::types::{
    BoxFuture, CompactOptions, ExtensionApi, ExtensionContext, HandlerError, PrepareExecution, RestartOptions,
    SendUserMessageOptions, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, ToolRenderer,
    ToolResultRenderer, UserMessageContent, UserMessageDelivery,
};
use crate::core::permissions::presets::SandboxProfileName;
use crate::core::session_manager::{SessionInfo, SessionManager, lock_session_manager};
use crate::modes::interactive::theme::{Theme, ThemeColor, highlight_code};

use super::eval_tool::{PyrunEvalParams, PyrunPiRequestDispatcher, create_pyrun_eval_executor};
use super::runner::{PiRequest, PyrunRunnerClient, PyrunRunnerOptions, resolve_pyrun_runner_options};

const PYRUN_PROMPT_SNIPPET: &str = "Evaluate Python through the canonical Pyrun JSONL runtime adapter";
const PYRUN_PROMPT_GUIDELINES: [&str; 18] = [
    "Pyrun evaluates Python code in a persistent Python session with a persistent ctx object.",
    "Pi delegates Python/Pyrun runtime semantics to the Pyrun JSONL runner; Pi does not implement helper behavior locally.",
    "Do not use MCP for Pi's built-in pyrun_eval path; use the JSONL runner boundary.",
    "Use pi.footer.snapshot() to read the current Pi footer snapshot inside Pyrun.",
    "Use pi.compact(...) to trigger Pi session compaction from Pyrun.",
    "Use pi.restart(...) to restart Pi and resume the same session from Pyrun.",
    "Use pi.sessions.resume({ path | id | name }) to switch to a target session from Pyrun.",
    "Use pi.models.scoped() to list the current session scoped models for model cycling and pi.models.set(provider, model_id, thinking_level=None) to switch the current session model.",
    "Use pi.tools.call(name, params) to call active Pi tools from Pyrun, and pi.web_search(query) as a web_search shortcut.",
    "Use pi.commands.list() to list slash commands and pi.commands.run(name, args=\"\") to run registered slash commands from Pyrun.",
    "Use pi.agents.spawn(...), pi.agents.list(...), pi.agents.wait(), pi.agents.current(), pi.agents.select(agent_id), pi.messages.last(), pi.messages.enqueue(...), and pi.messages.send(...) for the supported Pi runtime bridge; pi.agents.wait() waits for any active agent and returns no agent output.",
    "Use Pyrun helpers directly: host, fs, cli, run, http, rg, fd, sqlite, kubectl, tools, text, seq, obj, and hr.",
    "When a bwrap sandbox profile is active, Pyrun executes inside that sandbox and Pi bridge helpers are unavailable.",
    "run.<program>(*args) executes immediately, sends stdout/stderr to the tool output by default, and returns ONLY the exit code (int). Example: `exit_code = run.git('status')`.",
    "cli.<program>(*args) returns a chainable CommandBuilder. cli.* builders forward output and return an exit code by default; use `.capture().run()` only when stdout/stderr or a structured CommandResult must be inspected. Example: `result = cli.git('status').capture().run()`.",
    "Use tools.ssh({ host, user, port, password }) for SSH commands that need password auth; it wraps sshpass automatically.",
    "run.* and cli.*.run() display output through the tool result and return only the exit code. Use cli.*.capture().run() for captured stdout/stderr and structured command results.",
    "Agents MUST NOT rerun the same command only to recover logs; inspect the saved CommandResult/full logs instead.",
];

pub const PYRUN_PROMPT_GUIDELINE_COUNT: usize = PYRUN_PROMPT_GUIDELINES.len();

#[derive(Clone, Default)]
pub struct PyrunExtensionOptions {
    pub background_jobs_store: Option<Arc<crate::core::multi_agent_store::MultiAgentStore>>,
    pub bwrap_command: Option<String>,
    pub detach_registry: Option<Arc<crate::core::tool_detach_registry::ToolDetachRegistry>>,
    pub pi_request_handlers: Vec<PyrunPiRequestDispatcher>,
}

pub type PyrunEvaluate = Arc<
    dyn Fn(
            PyrunEvalParams,
            ExtensionContext,
            Option<super::eval_tool::PyrunProgressUpdateCallback>,
            Option<tokio_util::sync::CancellationToken>,
            Option<String>,
        ) -> BoxFuture<'static, Result<AgentToolResult, HandlerError>>
        + Send
        + Sync,
>;

struct ExecutorState {
    evaluate: PyrunEvaluate,
    key: String,
    pi_bridge_enabled: bool,
    runner: PyrunRunnerClient,
    runner_options: PyrunRunnerOptions,
}

struct ExecutorSlot {
    state: Mutex<Option<ExecutorState>>,
}

impl ExecutorSlot {
    fn new() -> Self {
        Self {
            state: Mutex::new(None),
        }
    }
}

fn settings_profile(ctx: &ExtensionContext) -> SandboxProfileName {
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

fn process_env() -> indexmap::IndexMap<String, String> {
    std::env::vars().collect()
}

fn create_executor_state(
    ctx: &ExtensionContext,
    dispatch_pi_request: &PyrunPiRequestDispatcher,
    bwrap_command: &str,
) -> Result<ExecutorState, HandlerError> {
    let cwd = ctx.cwd().map_err(|error| HandlerError::new(error.to_string()))?;
    let profile = resolve_bwrap_sandbox_profile(settings_profile(ctx));
    let key = format!(
        "{}\0{cwd}",
        profile.map(|profile| profile.as_str()).unwrap_or("full-access")
    );
    let resolved = resolve_pyrun_runner_options(None).map_err(|error| HandlerError::new(error.to_string()))?;
    let Some(profile) = profile else {
        let runner_options = PyrunRunnerOptions::default();
        let runner = PyrunRunnerClient::new(runner_options.clone());
        let evaluate = create_pyrun_eval_executor(
            runner.clone(),
            Some(Arc::clone(dispatch_pi_request)),
            super::eval_tool::PyrunEvalExecutorOptions::default(),
        );
        return Ok(ExecutorState {
            evaluate,
            key,
            pi_bridge_enabled: true,
            runner,
            runner_options,
        });
    };
    let host_env = process_env();
    let runner_env = create_bwrap_runner_environment(
        &host_env,
        resolved
            .env
            .iter()
            .find(|(key, _)| key == "PYTHONPATH")
            .map(|(_, value)| value.as_str()),
    );
    let wrapped = create_bwrap_runner_command(BwrapRunnerCommandOptions {
        bwrap_command: bwrap_command.to_owned(),
        cwd,
        extra_read_only_paths: None,
        profile,
        runner_args: resolved.args,
        runner_command: resolved.command,
        runner_env: Some(runner_env),
    })
    .map_err(HandlerError::new)?;
    let runner_options = PyrunRunnerOptions {
        args: Some(wrapped.args),
        command: Some(wrapped.command),
        detached: None,
        env: Some(wrapped.env.into_iter().collect()),
        inherit_env: Some(false),
    };
    let runner = PyrunRunnerClient::new(runner_options.clone());
    let evaluate = create_pyrun_eval_executor(
        runner.clone(),
        None,
        super::eval_tool::PyrunEvalExecutorOptions {
            enable_pi_bridge: Some(false),
        },
    );
    Ok(ExecutorState {
        evaluate,
        key,
        pi_bridge_enabled: false,
        runner,
        runner_options,
    })
}

fn executor_for(
    slot: &ExecutorSlot,
    ctx: &ExtensionContext,
    dispatch_pi_request: &PyrunPiRequestDispatcher,
    bwrap_command: &str,
) -> Result<(PyrunEvaluate, bool, PyrunRunnerOptions), HandlerError> {
    let cwd = ctx.cwd().map_err(|error| HandlerError::new(error.to_string()))?;
    let profile = resolve_bwrap_sandbox_profile(settings_profile(ctx));
    let key = format!(
        "{}\0{cwd}",
        profile.map(|profile| profile.as_str()).unwrap_or("full-access")
    );
    let mut state = slot.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.as_ref().is_some_and(|state| state.key == key) {
        let state = state.as_ref().expect("state checked");
        return Ok((
            Arc::clone(&state.evaluate),
            state.pi_bridge_enabled,
            state.runner_options.clone(),
        ));
    }
    if let Some(old) = state.take() {
        old.runner.dispose();
    }
    let created = create_executor_state(ctx, dispatch_pi_request, bwrap_command)?;
    let result = (
        Arc::clone(&created.evaluate),
        created.pi_bridge_enabled,
        created.runner_options.clone(),
    );
    *state = Some(created);
    Ok(result)
}

fn dispose_executor(slot: &ExecutorSlot) {
    let old = slot
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some(old) = old {
        old.runner.dispose();
    }
}

fn read_string_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn format_pyrun_result(text: &str, executed: Option<&str>, is_error: bool, theme: &Theme) -> String {
    let Some(executed) = executed else {
        return theme.fg(ThemeColor::ToolOutput, text);
    };
    if !text.starts_with(executed) {
        let prefix = if is_error && !text.starts_with("Error:") {
            "Error: "
        } else {
            ""
        };
        return theme.fg(ThemeColor::ToolOutput, &format!("{prefix}{text}"));
    }
    let remainder = text[executed.len()..].trim_start_matches('\n');
    theme.fg(ThemeColor::ToolOutput, remainder)
}

fn get_executed_code(details: Option<&Value>) -> Option<String> {
    details?.get("executed").and_then(Value::as_str).map(str::to_owned)
}

fn normalize_restart_params(params: Option<&Value>) -> Result<RestartOptions, HandlerError> {
    let Some(params) = params else {
        return Ok(RestartOptions {
            notice: None,
            process: Some(true),
        });
    };
    if params.is_null() {
        return Ok(RestartOptions {
            notice: None,
            process: Some(true),
        });
    }
    if let Some(notice) = params.as_str() {
        return Ok(RestartOptions {
            notice: Some(notice.to_owned()),
            process: Some(true),
        });
    }
    let Some(record) = params.as_object() else {
        return Err(HandlerError::new(
            "pi.restart requires no argument, a notice string, or { notice } object",
        ));
    };
    let notice = match record.get("notice") {
        Some(Value::String(notice)) => Some(notice.clone()),
        Some(_) => return Err(HandlerError::new("pi.restart notice must be a string")),
        None => None,
    };
    Ok(RestartOptions {
        notice,
        process: Some(true),
    })
}

fn normalize_compact_params(params: Option<&Value>) -> Result<CompactOptions, HandlerError> {
    let Some(params) = params else {
        return Ok(CompactOptions::default());
    };
    if params.is_null() {
        return Ok(CompactOptions::default());
    }
    if let Some(custom_instructions) = params.as_str() {
        return Ok(CompactOptions {
            custom_instructions: Some(custom_instructions.to_owned()),
            on_complete: None,
            on_error: None,
        });
    }
    let Some(record) = params.as_object() else {
        return Err(HandlerError::new(
            "pi.compact requires no argument, a custom instructions string, or { customInstructions } object",
        ));
    };
    let custom_instructions = match record.get("customInstructions") {
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return Err(HandlerError::new("pi.compact customInstructions must be a string")),
        None => None,
    };
    Ok(CompactOptions {
        custom_instructions,
        on_complete: None,
        on_error: None,
    })
}

#[derive(Debug, Clone, Default)]
struct ResumeSessionParams {
    id: Option<String>,
    name: Option<String>,
    path: Option<String>,
}

fn read_non_empty_string(record: &Map<String, Value>, key: &str) -> Result<Option<String>, HandlerError> {
    let Some(value) = record.get(key) else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(HandlerError::new(format!(
            "pi.sessions.resume {key} must be a non-empty string"
        )));
    };
    let trimmed = pi_js_compat::string::js_trim(value);
    if trimmed.is_empty() {
        return Err(HandlerError::new(format!(
            "pi.sessions.resume {key} must be a non-empty string"
        )));
    }
    Ok(Some(trimmed.to_owned()))
}

fn normalize_resume_session_params(params: Option<&Value>) -> Result<ResumeSessionParams, HandlerError> {
    let Some(Value::Object(record)) = params else {
        return Err(HandlerError::new(
            "pi.sessions.resume requires { path }, { id }, or { name }",
        ));
    };
    let target = ResumeSessionParams {
        id: read_non_empty_string(record, "id")?,
        name: read_non_empty_string(record, "name")?,
        path: read_non_empty_string(record, "path")?,
    };
    let target_count = [&target.id, &target.name, &target.path]
        .into_iter()
        .filter(|value| value.is_some())
        .count();
    if target_count != 1 {
        return Err(HandlerError::new(
            "pi.sessions.resume requires exactly one of path, id, or name",
        ));
    }
    Ok(target)
}

fn assert_resume_session_path(path: &str) -> Result<String, HandlerError> {
    if !Path::new(path).is_file() {
        return Err(HandlerError::new(format!("Session file does not exist: {path}")));
    }
    if !path.ends_with(".jsonl") {
        return Err(HandlerError::new(format!("Session file must be a .jsonl file: {path}")));
    }
    Ok(path.to_owned())
}

fn find_unique_session_match(
    sessions: &[SessionInfo],
    label: &str,
    matches: impl Fn(&SessionInfo) -> bool,
) -> Result<SessionInfo, HandlerError> {
    let matched: Vec<&SessionInfo> = sessions.iter().filter(|session| matches(session)).collect();
    match matched.len() {
        0 => Err(HandlerError::new(format!("No session found matching {label}"))),
        1 => Ok(matched[0].clone()),
        _ => Err(HandlerError::new(format!("Ambiguous session match for {label}"))),
    }
}

fn list_resolvable_sessions(ctx: &ExtensionContext) -> Result<Vec<SessionInfo>, HandlerError> {
    let session = ctx
        .session_manager()
        .map_err(|error| HandlerError::new(error.to_string()))?;
    let (cwd, session_dir) = {
        let manager = lock_session_manager(&session);
        (manager.get_cwd().to_owned(), manager.get_session_dir().to_owned())
    };
    let local = SessionManager::list(&cwd, Some(&session_dir), None, None)
        .map_err(|error| HandlerError::new(error.to_string()))?;
    let all = SessionManager::list_all(None, None, None).map_err(|error| HandlerError::new(error.to_string()))?;
    let mut merged: indexmap::IndexMap<String, SessionInfo> = indexmap::IndexMap::new();
    for session in local.into_iter().chain(all) {
        merged.insert(session.path.clone(), session);
    }
    Ok(merged.into_values().collect())
}

fn resolve_resume_session_file(params: &ResumeSessionParams, ctx: &ExtensionContext) -> Result<String, HandlerError> {
    if let Some(path) = &params.path {
        let cwd = ctx.cwd().map_err(|error| HandlerError::new(error.to_string()))?;
        let resolved = crate::utils::paths::resolve_path(path, &cwd, &Default::default())
            .map_err(|error| HandlerError::new(error.to_string()))?;
        return assert_resume_session_path(&resolved);
    }
    let sessions = list_resolvable_sessions(ctx)?;
    if let Some(name) = &params.name {
        return Ok(
            find_unique_session_match(&sessions, &format!("name '{name}'"), |session| {
                session.name.as_deref() == Some(name)
            })?
            .path,
        );
    }
    let Some(id) = &params.id else {
        return Err(HandlerError::new(
            "pi.sessions.resume requires { path }, { id }, or { name }",
        ));
    };
    let exact_matches: Vec<&SessionInfo> = sessions.iter().filter(|session| &session.id == id).collect();
    if exact_matches.len() == 1 {
        return Ok(exact_matches[0].path.clone());
    }
    if exact_matches.len() > 1 {
        return Err(HandlerError::new(format!("Ambiguous session match for id '{id}'")));
    }
    Ok(find_unique_session_match(&sessions, &format!("id '{id}'"), |session| session.id.starts_with(id))?.path)
}

fn list_scoped_models(ctx: &ExtensionContext) -> Result<Value, HandlerError> {
    let scoped = ctx
        .scoped_models()
        .map_err(|error| HandlerError::new(error.to_string()))?;
    let models: Vec<Value> = scoped
        .into_iter()
        .map(|scoped| {
            let mut model = Map::new();
            model.insert("id".to_owned(), Value::String(scoped.model.id.clone()));
            if !scoped.model.name.is_empty() {
                model.insert("name".to_owned(), Value::String(scoped.model.name.clone()));
            }
            model.insert("provider".to_owned(), Value::String(scoped.model.provider.to_string()));
            if let Some(thinking_level) = scoped.thinking_level {
                model.insert(
                    "thinkingLevel".to_owned(),
                    Value::String(thinking_level.as_str().to_owned()),
                );
            }
            Value::Object(model)
        })
        .collect();
    Ok(Value::Array(models))
}

fn parse_thinking_level(value: &str) -> Option<ThinkingLevel> {
    match value {
        "off" => Some(ThinkingLevel::Off),
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" => Some(ThinkingLevel::XHigh),
        "max" => Some(ThinkingLevel::Max),
        "ultra" => Some(ThinkingLevel::Ultra),
        _ => None,
    }
}

async fn set_session_model(
    params: Option<&Value>,
    pi: &ExtensionApi,
    ctx: &ExtensionContext,
) -> Result<Value, HandlerError> {
    let Some(Value::Object(record)) = params else {
        return Err(HandlerError::new(
            "pi.models.set requires { provider, id, thinkingLevel? }",
        ));
    };
    let provider = record.get("provider").and_then(Value::as_str).unwrap_or_default();
    if pi_js_compat::string::js_trim(provider).is_empty() {
        return Err(HandlerError::new("pi.models.set requires a non-empty provider"));
    }
    let id = record.get("id").and_then(Value::as_str).unwrap_or_default();
    if pi_js_compat::string::js_trim(id).is_empty() {
        return Err(HandlerError::new("pi.models.set requires a non-empty id"));
    }
    let thinking_level = match record.get("thinkingLevel") {
        Some(Value::String(value)) => Some(parse_thinking_level(value).ok_or_else(|| {
            HandlerError::new(
                "pi.models.set thinkingLevel must be off, minimal, low, medium, high, xhigh, max, or ultra",
            )
        })?),
        Some(_) => {
            return Err(HandlerError::new(
                "pi.models.set thinkingLevel must be off, minimal, low, medium, high, xhigh, max, or ultra",
            ));
        }
        None => None,
    };
    let provider = pi_js_compat::string::js_trim(provider).to_owned();
    let id = pi_js_compat::string::js_trim(id).to_owned();
    let model = ctx
        .model_registry()
        .map_err(|error| HandlerError::new(error.to_string()))?
        .get_available()
        .into_iter()
        .find(|model| model.provider.as_str() == provider && model.id == id)
        .ok_or_else(|| HandlerError::new(format!("Model not found or not authenticated: {provider}/{id}")))?;
    if !pi
        .set_model(model.clone())
        .await
        .map_err(|error| HandlerError::new(error.to_string()))?
    {
        return Err(HandlerError::new(format!("No API key for {provider}/{id}")));
    }
    if let Some(level) = thinking_level {
        pi.set_thinking_level(level)
            .map_err(|error| HandlerError::new(error.to_string()))?;
    }
    let mut result = Map::new();
    result.insert("model".to_owned(), serde_json::to_value(&model).unwrap_or(Value::Null));
    if let Some(level) = thinking_level {
        result.insert("thinkingLevel".to_owned(), Value::String(level.as_str().to_owned()));
    }
    Ok(Value::Object(result))
}

fn normalize_tool_call_params(params: Option<&Value>) -> Result<(String, Value), HandlerError> {
    let Some(Value::Object(record)) = params else {
        return Err(HandlerError::new("pi.tools.call requires { name, params }"));
    };
    let name = record.get("name").or_else(|| record.get("toolName"));
    let Some(name) = name.and_then(Value::as_str) else {
        return Err(HandlerError::new("pi.tools.call requires a non-empty tool name"));
    };
    let name = pi_js_compat::string::js_trim(name);
    if name.is_empty() {
        return Err(HandlerError::new("pi.tools.call requires a non-empty tool name"));
    }
    Ok((name.to_owned(), record.get("params").cloned().unwrap_or(Value::Null)))
}

fn normalize_command_run_params(params: Option<&Value>) -> Result<(String, String), HandlerError> {
    let Some(Value::Object(record)) = params else {
        return Err(HandlerError::new("pi.commands.run requires { name, args }"));
    };
    let Some(name) = record.get("name").and_then(Value::as_str) else {
        return Err(HandlerError::new("pi.commands.run requires a non-empty command name"));
    };
    let name = pi_js_compat::string::js_trim(name);
    if name.is_empty() {
        return Err(HandlerError::new("pi.commands.run requires a non-empty command name"));
    }
    let args = match record.get("args") {
        Some(Value::String(args)) => args.clone(),
        Some(_) => return Err(HandlerError::new("pi.commands.run args must be a string when provided")),
        None => String::new(),
    };
    Ok((name.to_owned(), args))
}

fn normalize_message_params(params: Option<&Value>) -> Result<(String, Option<UserMessageDelivery>), HandlerError> {
    let Some(params) = params else {
        return Err(HandlerError::new(
            "pi.messages.enqueue requires a message string or { message } object",
        ));
    };
    if let Some(message) = params.as_str() {
        return Ok((message.to_owned(), None));
    }
    let Some(record) = params.as_object() else {
        return Err(HandlerError::new(
            "pi.messages.enqueue requires a message string or { message } object",
        ));
    };
    let Some(message) = record.get("message").and_then(Value::as_str) else {
        return Err(HandlerError::new("pi.messages.enqueue requires a string message"));
    };
    let deliver_as = match record.get("deliverAs").and_then(Value::as_str) {
        Some("steer") => Some(UserMessageDelivery::Steer),
        Some("followUp") => Some(UserMessageDelivery::FollowUp),
        _ => None,
    };
    Ok((message.to_owned(), deliver_as))
}

async fn dispatch_builtin_pyrun_request(
    request: &PiRequest,
    pi: &ExtensionApi,
    ctx: &ExtensionContext,
    signal: Option<tokio_util::sync::CancellationToken>,
    active_tool_call_id: Option<&str>,
) -> Result<Option<Value>, HandlerError> {
    match request.method.as_str() {
        "models.scoped" => Ok(Some(list_scoped_models(ctx)?)),
        "models.set" => Ok(Some(set_session_model(Some(&request.params), pi, ctx).await?)),
        "tools.call" => {
            let (name, params) = normalize_tool_call_params(Some(&request.params))?;
            let result = pi
                .call_tool(name, params, signal, active_tool_call_id.map(str::to_owned))
                .await?;
            Ok(Some(serde_json::to_value(&result).unwrap_or(Value::Null)))
        }
        "commands.list" => Ok(Some(serde_json::to_value(pi.get_commands()?).unwrap_or(Value::Null))),
        "commands.run" => {
            let (name, args) = normalize_command_run_params(Some(&request.params))?;
            Ok(Some(pi.call_command(name, Some(args)).await?))
        }
        "compact" => {
            ctx.compact(Some(normalize_compact_params(Some(&request.params))?))?;
            Ok(Some(json!({"started": true})))
        }
        "messages.enqueue" => {
            let (message, deliver_as) = normalize_message_params(Some(&request.params))?;
            pi.send_user_message(
                UserMessageContent::Text(message),
                Some(SendUserMessageOptions {
                    deliver_as,
                    expand_prompt_templates: None,
                }),
            )?;
            Ok(Some(json!({"enqueued": true})))
        }
        "restart" => {
            let future = ctx.restart(Some(normalize_restart_params(Some(&request.params))?))?;
            future.await;
            Ok(Some(json!({"started": true})))
        }
        "sessions.resume" => {
            let params = normalize_resume_session_params(Some(&request.params))?;
            let session_file = resolve_resume_session_file(&params, ctx)?;
            let result = ctx.switch_session(session_file, None).await?;
            Ok(Some(json!({
                "cancelled": result.cancelled,
                "resumed": !result.cancelled
            })))
        }
        _ => Ok(None),
    }
}

/// `createPyrunPiDispatcher`.
pub fn create_pyrun_pi_dispatcher(pi: &ExtensionApi, options: &PyrunExtensionOptions) -> PyrunPiRequestDispatcher {
    let pi = pi.clone();
    let handlers = options.pi_request_handlers.clone();
    Arc::new(move |request, ctx, signal, active_tool_call_id| {
        let pi = pi.clone();
        let handlers = handlers.clone();
        Box::pin(async move {
            if let Some(result) =
                dispatch_builtin_pyrun_request(&request, &pi, &ctx, signal.clone(), active_tool_call_id.as_deref())
                    .await?
            {
                return Ok(result);
            }
            for handler in &handlers {
                let result = handler(
                    request.clone(),
                    ctx.clone(),
                    signal.clone(),
                    active_tool_call_id.clone(),
                )
                .await?;
                if !result.is_null() {
                    return Ok(result);
                }
            }
            Err(HandlerError::new(format!(
                "Pi capability is unavailable: {}",
                request.method
            )))
        }) as BoxFuture<'static, Result<Value, HandlerError>>
    })
}

fn pyrun_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "code": {"type": "string", "description": "Python source to evaluate."},
            "session_id": {"type": "string", "description": "Pyrun session id. Defaults to this Pi session."}
        },
        "required": ["code"]
    })
}

fn update_existing_text(context: &ToolRenderContext, text: String) -> Option<ComponentRef> {
    let component = context.last_component.clone()?;
    {
        let mut borrowed = component.try_borrow_mut().ok()?;
        let existing = borrowed.as_any_mut()?.downcast_mut::<Text>()?;
        existing.set_text(text);
    }
    Some(component)
}

fn render_call(args: &Value, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    let session_id = read_string_arg(args, "session_id");
    let label = match session_id {
        Some(session_id) if session_id != "default" => format!("pyrun_eval({session_id})"),
        _ => "pyrun_eval".to_owned(),
    };
    let code = read_string_arg(args, "code");
    let highlighted = code
        .as_deref()
        .map(|code| highlight_code(code, Some("python")).join("\n"));
    let text = match highlighted {
        Some(highlighted) => format!("{}\n{highlighted}", theme.bold(&label)),
        None => theme.bold(&label),
    };
    if let Some(component) = update_existing_text(context, text.clone()) {
        return component;
    }
    std::rc::Rc::new(std::cell::RefCell::new(Text::new(text, 0, 0))) as ComponentRef
}

fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    let output = result
        .content
        .iter()
        .filter_map(|item| match item {
            TextOrImageContent::Text(text) => Some(text.text.as_str()),
            TextOrImageContent::Image(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let executed = get_executed_code(result.details.as_ref()).or_else(|| read_string_arg(&context.args, "code"));
    let text = format_pyrun_result(&output, executed.as_deref(), context.is_error, theme);
    if let Some(component) = update_existing_text(context, text.clone()) {
        return component;
    }
    std::rc::Rc::new(std::cell::RefCell::new(Text::new(text, 0, 0))) as ComponentRef
}

/// `createPyrunToolDefinition`.
pub fn create_pyrun_tool_definition(evaluate: PyrunEvaluate) -> ToolDefinition {
    let execute = Arc::new(
        move |_tool_call_id: String, params: Value, signal, on_update, ctx: ExtensionContext| {
            let evaluate = Arc::clone(&evaluate);
            Box::pin(async move {
                let params = PyrunEvalParams {
                    code: params
                        .get("code")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    session_id: params.get("session_id").and_then(Value::as_str).map(str::to_owned),
                };
                evaluate(params, ctx, on_update, signal, Some(_tool_call_id)).await
            }) as BoxFuture<'static, Result<AgentToolResult, HandlerError>>
        },
    );
    let prepare_execution: PrepareExecution =
        Arc::new(|_tool_call_id, _signal, _ctx| Box::pin(async { Ok(Some(ToolCallExecution { agent_id: uuidv7() })) }));
    let render_call: ToolRenderer = Arc::new(render_call);
    let render_result: ToolResultRenderer = Arc::new(render_result);
    ToolDefinition {
        name: "pyrun_eval".to_owned(),
        label: "Pyrun Eval".to_owned(),
        description: "Evaluate Python/Pyrun code through the canonical Pyrun JSONL runtime adapter.".to_owned(),
        prompt_snippet: Some(PYRUN_PROMPT_SNIPPET.to_owned()),
        prompt_guidelines: Some(PYRUN_PROMPT_GUIDELINES.iter().map(|line| (*line).to_owned()).collect()),
        approval_required: Some(true),
        approval_kind: None,
        parameters: pyrun_parameters(),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: Some(prepare_execution),
        execution_mode: None,
        execute,
        render_call: Some(render_call),
        render_result: Some(render_result),
    }
}

/// `pyrunExtension`.
pub fn install_pyrun_extension(pi: &ExtensionApi, options: PyrunExtensionOptions) -> Result<(), HandlerError> {
    let dispatch_pi_request = create_pyrun_pi_dispatcher(pi, &options);
    let bwrap_command = options
        .bwrap_command
        .clone()
        .or_else(|| std::env::var("PI_BWRAP_COMMAND").ok())
        .unwrap_or_else(|| "bwrap".to_owned());
    let slot = Arc::new(ExecutorSlot::new());
    let slot_for_evaluate = Arc::clone(&slot);
    let dispatch_for_evaluate = Arc::clone(&dispatch_pi_request);
    let options_for_evaluate = options.clone();
    let evaluate: PyrunEvaluate = Arc::new(move |params, ctx, on_update, signal, tool_call_id| {
        let slot = Arc::clone(&slot_for_evaluate);
        let dispatch = Arc::clone(&dispatch_for_evaluate);
        let options = options_for_evaluate.clone();
        let bwrap_command = bwrap_command.clone();
        Box::pin(async move {
            let (evaluate, _pi_bridge, _runner_options) = executor_for(&slot, &ctx, &dispatch, &bwrap_command)?;
            let store = options
                .background_jobs_store
                .clone()
                .or(ctx.multi_agent_store().ok().flatten());
            let detach_registry = options
                .detach_registry
                .clone()
                .or(ctx.tool_detach_registry().ok().flatten());
            if store.is_none() || detach_registry.is_none() {
                return evaluate(params, ctx, on_update, signal, tool_call_id).await;
            }
            super::detached_evaluation::run_durable_detachable_pyrun_evaluation(
                super::detached_evaluation::DurablePyrunEvaluationInput {
                    agent_id: ctx.tool_execution_agent_id.clone(),
                    ctx,
                    detach_registry: detach_registry.expect("checked"),
                    dispatch_pi_request: dispatch,
                    on_update,
                    params,
                    pi_bridge_enabled: _pi_bridge,
                    tool_call_id,
                    runner_options: _runner_options,
                    signal,
                    store: store.expect("checked"),
                },
            )
            .await
        }) as BoxFuture<'static, Result<AgentToolResult, HandlerError>>
    });
    pi.register_tool(create_pyrun_tool_definition(evaluate))?;
    let slot_for_shutdown = Arc::clone(&slot);
    let handlers_for_shutdown = options.pi_request_handlers.clone();
    pi.on_session_shutdown(move |_event, _ctx| {
        let slot = Arc::clone(&slot_for_shutdown);
        let handlers = handlers_for_shutdown.clone();
        Box::pin(async move {
            dispose_executor(&slot);
            for handler in handlers {
                let _ = handler;
            }
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    let _ = &dispatch_pi_request;
    Ok(())
}

/// Registered factory for `<first-party:pyrun>`.
pub fn pyrun_extension_factory(options: PyrunExtensionOptions) -> ExtensionFactory {
    Arc::new(move |pi| {
        let result = install_pyrun_extension(pi, options.clone());
        Box::pin(async move { result })
    })
}
