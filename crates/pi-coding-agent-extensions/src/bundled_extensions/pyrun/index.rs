//! Port of `packages/coding-agent/extensions/pyrun/src/index.ts` (non-detached
//! paths plus the shared dispatcher and tool definition).

use pi_js_compat::js_format;
use std::path::Path;
use std::sync::{Arc, Mutex};

use pi_agent_core::AgentToolResult;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_agent_core::uuidv7;
use pi_ai::types::{TextOrImageContent, ToolCallExecution};
use pi_js_compat::{
    js_json as json,
    js_json::{self, JsObject as Map, JsString, JsValue as Value, encode_node_utf8},
};
use pi_tui::components::text::Text;
use pi_tui::tui::ComponentRef;

use crate::bundled_extensions::bwrap::backend::{
    BwrapRunnerCommandOptions, create_bwrap_runner_command, create_bwrap_runner_environment,
    resolve_bwrap_sandbox_profile,
};
use crate::core::extensions::loader::ExtensionFactory;
use crate::core::extensions::types::{
    BoxFuture, CompactOptions, ExtensionApi, ExtensionContext, HandlerError, PrepareExecution, RestartOptions,
    SendUserMessageOptions, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, ToolRenderer,
    ToolResultRenderer, UserMessageContent, UserMessageDelivery, infallible_render_result,
};
use crate::core::permissions::presets::SandboxProfileName;
use crate::core::session_manager::{SessionInfo, SessionManager};
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
    pub bwrap_command: Option<JsString>,
    pub detach_registry: Option<Arc<crate::core::tool_detach_registry::ToolDetachRegistry>>,
    pub pi_request_handlers: Vec<PyrunPiRequestDispatcher>,
}

pub type PyrunEvaluate = Arc<
    dyn Fn(
            PyrunEvalParams,
            ExtensionContext,
            Option<super::eval_tool::PyrunProgressUpdateCallback>,
            Option<pi_js_compat::abort::AbortSignal>,
            Option<String>,
        ) -> BoxFuture<'static, Result<AgentToolResult, HandlerError>>
        + Send
        + Sync,
>;

struct ExecutorState {
    evaluate: PyrunEvaluate,
    key: JsString,
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

fn process_env() -> indexmap::IndexMap<JsString, JsString> {
    pi_js_compat::process_env::vars()
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect()
}

fn create_executor_state(
    ctx: &ExtensionContext,
    dispatch_pi_request: &PyrunPiRequestDispatcher,
    bwrap_command: &JsString,
) -> Result<ExecutorState, HandlerError> {
    let cwd = ctx.cwd()?;
    let profile = resolve_bwrap_sandbox_profile(settings_profile(ctx));
    let key = js_format!(
        "{}\0{}",
        profile.map(|profile| profile.as_str()).unwrap_or("full-access"),
        cwd
    );
    let resolved = resolve_pyrun_runner_options(None).map_err(|error| HandlerError::new(error.message()))?;
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
            .map(|(_, value)| value),
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
    bwrap_command: &JsString,
) -> Result<(PyrunEvaluate, bool, PyrunRunnerOptions), HandlerError> {
    let cwd = ctx.cwd()?;
    let profile = resolve_bwrap_sandbox_profile(settings_profile(ctx));
    let key = js_format!(
        "{}\0{}",
        profile.map(|profile| profile.as_str()).unwrap_or("full-access"),
        cwd
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

fn read_string_arg(args: &Value, key: &str) -> Option<JsString> {
    args.get(key).and_then(Value::as_string).cloned()
}

fn format_pyrun_result(text: &JsString, executed: Option<&JsString>, is_error: bool, theme: &Theme) -> JsString {
    let Some(executed) = executed else {
        return theme.fg(ThemeColor::ToolOutput, text);
    };
    if !text.starts_with(executed) {
        let prefix = if is_error && !text.starts_with("Error:") {
            "Error: "
        } else {
            ""
        };
        return theme.fg(ThemeColor::ToolOutput, js_format!("{}{}", prefix, text));
    }
    let newlines = text.units()[executed.len()..]
        .iter()
        .take_while(|unit| **unit == u16::from(b'\n'))
        .count();
    theme.fg(
        ThemeColor::ToolOutput,
        text.slice(executed.len() + newlines, text.len()),
    )
}

fn get_executed_code(details: Option<&Value>) -> Option<JsString> {
    details?.get("executed").and_then(Value::as_string).cloned()
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
    if let Some(notice) = params.as_string() {
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
    if let Some(custom_instructions) = params.as_string() {
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
    id: Option<JsString>,
    name: Option<JsString>,
    path: Option<JsString>,
}

fn read_non_empty_string(record: &Map, key: &str) -> Result<Option<JsString>, HandlerError> {
    let Some(value) = record.get(key) else {
        return Ok(None);
    };
    let Some(value) = value.as_string() else {
        return Err(HandlerError::new(format!(
            "pi.sessions.resume {key} must be a non-empty string"
        )));
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(HandlerError::new(format!(
            "pi.sessions.resume {key} must be a non-empty string"
        )));
    }
    Ok(Some(trimmed))
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    #[test]
    fn result_renderer_strips_executed_units_without_repairing_remaining_output() {
        let theme_path =
            JsString::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent")) + "/theme/dark.json";
        let theme = crate::modes::interactive::theme::load_theme_from_path(
            &theme_path,
            Some(crate::modes::interactive::theme::ColorMode::Truecolor),
        )
        .unwrap();
        let executed = JsString::from("code-") + JsString::from_utf16(vec![0xd800]);
        let output = JsString::from("output-") + JsString::from_utf16(vec![0xdfff]);
        let rendered = format_pyrun_result(&(executed.clone() + "\n\n" + &output), Some(&executed), false, &theme);
        assert_eq!(crate::utils::ansi::strip_ansi(&rendered), output);
    }

    #[test]
    fn resume_name_remains_a_utf16_lookup_value() {
        let params = js_json::parse_lossless(r#"{"name":"session-\ud800"}"#).unwrap();
        let normalized = normalize_resume_session_params(Some(&params)).unwrap();
        assert_eq!(normalized.name, params["name"].as_string().cloned());
    }
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

fn assert_resume_session_path(path: &JsString) -> Result<JsString, HandlerError> {
    if !Path::new(&encode_node_utf8(path)).is_file() {
        return Err(HandlerError::new(js_format!("Session file does not exist: {}", path)));
    }
    if !path.ends_with(".jsonl") {
        return Err(HandlerError::new(js_format!(
            "Session file must be a .jsonl file: {}",
            path
        )));
    }
    Ok(path.to_owned())
}

fn find_unique_session_match(
    sessions: &[SessionInfo],
    label: &JsString,
    matches: impl Fn(&SessionInfo) -> bool,
) -> Result<SessionInfo, HandlerError> {
    let matched: Vec<&SessionInfo> = sessions.iter().filter(|session| matches(session)).collect();
    match matched.len() {
        0 => Err(HandlerError::new(JsString::from("No session found matching ") + label)),
        1 => Ok(matched[0].clone()),
        _ => Err(HandlerError::new(
            JsString::from("Ambiguous session match for ") + label,
        )),
    }
}

fn list_resolvable_sessions(ctx: &ExtensionContext) -> Result<Vec<SessionInfo>, HandlerError> {
    let session = ctx.session_manager()?;
    let (cwd, session_dir) = {
        let manager = session;
        (manager.get_cwd().to_owned(), manager.get_session_dir().to_owned())
    };
    let local = SessionManager::list(&cwd, Some(&session_dir), None, None)
        .map_err(|error| HandlerError::new(error.message()))?;
    let all = SessionManager::list_all(None, None, None).map_err(|error| HandlerError::new(error.message()))?;
    let mut merged: indexmap::IndexMap<JsString, SessionInfo> = indexmap::IndexMap::new();
    for session in local.into_iter().chain(all) {
        merged.insert(session.path.clone(), session);
    }
    Ok(merged.into_values().collect())
}

fn resolve_resume_session_file(params: &ResumeSessionParams, ctx: &ExtensionContext) -> Result<JsString, HandlerError> {
    if let Some(path) = &params.path {
        let cwd = ctx.cwd()?;
        let resolved = crate::utils::paths::resolve_path(path, &cwd, &Default::default())
            .map_err(|error| HandlerError::new(error.message()))?;
        return assert_resume_session_path(&resolved);
    }
    let sessions = list_resolvable_sessions(ctx)?;
    if let Some(name) = &params.name {
        return Ok(
            find_unique_session_match(&sessions, &(JsString::from("name '") + name + "'"), |session| {
                session.name.as_ref() == Some(name)
            })?
            .path,
        );
    }
    let Some(id) = &params.id else {
        return Err(HandlerError::new(
            "pi.sessions.resume requires { path }, { id }, or { name }",
        ));
    };
    let exact_matches: Vec<&SessionInfo> = sessions.iter().filter(|session| id == &session.id).collect();
    if exact_matches.len() == 1 {
        return Ok(exact_matches[0].path.clone());
    }
    if exact_matches.len() > 1 {
        return Err(HandlerError::new(
            JsString::from("Ambiguous session match for id '") + id + "'",
        ));
    }
    Ok(
        find_unique_session_match(&sessions, &(JsString::from("id '") + id + "'"), |session| {
            session.id.starts_with(id)
        })?
        .path,
    )
}

fn list_scoped_models(ctx: &ExtensionContext) -> Result<Value, HandlerError> {
    let scoped = ctx.scoped_models()?;
    let models: Vec<Value> = scoped
        .into_iter()
        .map(|scoped| {
            let mut model = Map::new();
            model.insert("id".into(), Value::String(scoped.model.id.clone().into()));
            if !scoped.model.name.is_empty() {
                model.insert("name".into(), Value::String(scoped.model.name.clone().into()));
            }
            model.insert(
                "provider".into(),
                Value::String(scoped.model.provider.to_string().into()),
            );
            if let Some(thinking_level) = scoped.thinking_level {
                model.insert(
                    "thinkingLevel".into(),
                    Value::String(thinking_level.as_str().to_owned().into()),
                );
            }
            Value::Object(model)
        })
        .collect();
    Ok(Value::Array(models))
}

fn parse_thinking_level(value: &JsString) -> Option<ThinkingLevel> {
    match value.to_utf8().as_deref() {
        Ok("off") => Some(ThinkingLevel::Off),
        Ok("minimal") => Some(ThinkingLevel::Minimal),
        Ok("low") => Some(ThinkingLevel::Low),
        Ok("medium") => Some(ThinkingLevel::Medium),
        Ok("high") => Some(ThinkingLevel::High),
        Ok("xhigh") => Some(ThinkingLevel::XHigh),
        Ok("max") => Some(ThinkingLevel::Max),
        Ok("ultra") => Some(ThinkingLevel::Ultra),
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
    let provider = record
        .get("provider")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_default();
    if provider.trim().is_empty() {
        return Err(HandlerError::new("pi.models.set requires a non-empty provider"));
    }
    let id = record.get("id").and_then(Value::as_string).cloned().unwrap_or_default();
    if id.trim().is_empty() {
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
    let provider = provider.trim().to_owned();
    let id = id.trim().to_owned();
    let model = ctx
        .model_registry()?
        .get_available()?
        .into_iter()
        .find(|model| provider == model.provider.as_str() && id == model.id)
        .ok_or_else(|| {
            HandlerError::new(JsString::from("Model not found or not authenticated: ") + &provider + "/" + &id)
        })?;
    if !pi.set_model(model.clone()).await? {
        return Err(HandlerError::new(
            JsString::from("No API key for ") + &provider + "/" + &id,
        ));
    }
    if let Some(level) = thinking_level {
        pi.set_thinking_level(level)?;
    }
    let mut result = Map::new();
    result.insert("model".into(), js_json::to_value(&model).unwrap_or(Value::Null));
    if let Some(level) = thinking_level {
        result.insert("thinkingLevel".into(), Value::String(level.as_str().to_owned().into()));
    }
    Ok(Value::Object(result))
}

fn normalize_tool_call_params(params: Option<&Value>) -> Result<(String, Value), HandlerError> {
    let Some(Value::Object(record)) = params else {
        return Err(HandlerError::new("pi.tools.call requires { name, params }"));
    };
    let name = record.get("name").or_else(|| record.get("toolName"));
    let Some(name) = name.and_then(Value::as_string) else {
        return Err(HandlerError::new("pi.tools.call requires a non-empty tool name"));
    };
    let name = name.trim();
    if name.is_empty() {
        return Err(HandlerError::new("pi.tools.call requires a non-empty tool name"));
    }
    Ok((
        name.to_utf8().map_err(|error| HandlerError::new(error.to_string()))?,
        record.get("params").cloned().unwrap_or(Value::Null),
    ))
}

fn normalize_command_run_params(params: Option<&Value>) -> Result<(JsString, JsString), HandlerError> {
    let Some(Value::Object(record)) = params else {
        return Err(HandlerError::new("pi.commands.run requires { name, args }"));
    };
    let Some(name) = record.get("name").and_then(Value::as_string) else {
        return Err(HandlerError::new("pi.commands.run requires a non-empty command name"));
    };
    let name = name.trim();
    if name.is_empty() {
        return Err(HandlerError::new("pi.commands.run requires a non-empty command name"));
    }
    let args = match record.get("args") {
        Some(Value::String(args)) => args.clone(),
        Some(_) => return Err(HandlerError::new("pi.commands.run args must be a string when provided")),
        None => JsString::default(),
    };
    Ok((name, args))
}

fn normalize_message_params(params: Option<&Value>) -> Result<(JsString, Option<UserMessageDelivery>), HandlerError> {
    let Some(params) = params else {
        return Err(HandlerError::new(
            "pi.messages.enqueue requires a message string or { message } object",
        ));
    };
    if let Some(message) = params.as_string() {
        return Ok((message.to_owned(), None));
    }
    let Some(record) = params.as_object() else {
        return Err(HandlerError::new(
            "pi.messages.enqueue requires a message string or { message } object",
        ));
    };
    let Some(message) = record.get("message").and_then(Value::as_string) else {
        return Err(HandlerError::new("pi.messages.enqueue requires a string message"));
    };
    let deliver_as = match record.get("deliverAs").and_then(Value::as_string) {
        Some(value) if value == "steer" => Some(UserMessageDelivery::Steer),
        Some(value) if value == "followUp" => Some(UserMessageDelivery::FollowUp),
        _ => None,
    };
    Ok((message.to_owned(), deliver_as))
}

async fn dispatch_builtin_pyrun_request(
    request: &PiRequest,
    pi: &ExtensionApi,
    ctx: &ExtensionContext,
    signal: Option<pi_js_compat::abort::AbortSignal>,
    active_tool_call_id: Option<&str>,
) -> Result<Option<Value>, HandlerError> {
    match request.method.to_utf8().as_deref() {
        Ok("models.scoped") => Ok(Some(list_scoped_models(ctx)?)),
        Ok("models.set") => Ok(Some(set_session_model(Some(&request.params), pi, ctx).await?)),
        Ok("tools.call") => {
            let (name, params) = normalize_tool_call_params(Some(&request.params))?;
            let result = pi
                .call_tool(name, params.into(), signal, active_tool_call_id.map(str::to_owned))
                .await?;
            Ok(Some(js_json::to_value(&result).unwrap_or(Value::Null)))
        }
        Ok("commands.list") => Ok(Some(js_json::to_value(&pi.get_commands()?).unwrap_or(Value::Null))),
        Ok("commands.run") => {
            let (name, args) = normalize_command_run_params(Some(&request.params))?;
            Ok(Some(pi.call_command(name, Some(args)).await?.into()))
        }
        Ok("compact") => {
            ctx.compact(Some(normalize_compact_params(Some(&request.params))?))?;
            Ok(Some(json!({"started": true})))
        }
        Ok("messages.enqueue") => {
            let (message, deliver_as) = normalize_message_params(Some(&request.params))?;
            pi.send_user_message(
                UserMessageContent::Text(message.into()),
                Some(SendUserMessageOptions {
                    deliver_as,
                    expand_prompt_templates: None,
                }),
            )?;
            Ok(Some(json!({"enqueued": true})))
        }
        Ok("restart") => {
            let future = ctx.restart(Some(normalize_restart_params(Some(&request.params))?))?;
            future.await?;
            Ok(Some(json!({"started": true})))
        }
        Ok("sessions.resume") => {
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
                return Ok(Some(result));
            }
            for handler in &handlers {
                let result = handler(
                    request.clone(),
                    ctx.clone(),
                    signal.clone(),
                    active_tool_call_id.clone(),
                )
                .await?;
                if result.is_some() {
                    return Ok(result);
                }
            }
            Err(HandlerError::new(pi_js_compat::js_format!(
                "Pi capability is unavailable: {}",
                request.method
            )))
        }) as BoxFuture<'static, Result<Option<Value>, HandlerError>>
    })
}

fn pyrun_parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "code": {"type": "string", "description": "Python source to evaluate."},
            "session_id": {"type": "string", "description": "Pyrun session id. Defaults to this Pi session."}
        },
        "required": ["code"]
    })
}

fn update_existing_text(context: &ToolRenderContext, text: JsString) -> Option<ComponentRef> {
    let component = context.last_component.clone()?;
    {
        let mut borrowed = component.try_borrow_mut().ok()?;
        let existing = borrowed.as_any_mut()?.downcast_mut::<Text>()?;
        existing.set_text(text);
    }
    Some(component)
}

fn render_call(args: &Value, theme: &Theme, context: &ToolRenderContext) -> Result<ComponentRef, HandlerError> {
    let session_id = read_string_arg(args, "session_id");
    let label = match session_id {
        Some(session_id) if session_id != "default" => js_format!("pyrun_eval({})", session_id),
        _ => "pyrun_eval".into(),
    };
    let code = read_string_arg(args, "code");
    let highlighted = code
        .as_ref()
        .map(|code| highlight_code(code, Some(&"python".into())).map(|lines| JsString::join(lines, "\n")))
        .transpose()?;
    let text = match highlighted {
        Some(highlighted) => js_format!("{}\n{}", theme.bold(&label), highlighted),
        None => theme.bold(&label),
    };
    if let Some(component) = update_existing_text(context, text.clone()) {
        return Ok(component);
    }
    Ok(std::rc::Rc::new(std::cell::RefCell::new(Text::new(text, 0, 0))) as ComponentRef)
}

fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    let output = JsString::join(
        result
            .content
            .iter()
            .filter_map(|item| match item {
                TextOrImageContent::Text(text) => Some(text.text.clone()),
                TextOrImageContent::Image(_) => None,
            })
            .collect::<Vec<_>>(),
        "\n",
    );
    let executed = get_executed_code(result.details.as_ref()).or_else(|| read_string_arg(&context.args, "code"));
    let text = format_pyrun_result(&output, executed.as_ref(), context.is_error, theme);
    if let Some(component) = update_existing_text(context, text.clone()) {
        return component;
    }
    std::rc::Rc::new(std::cell::RefCell::new(Text::new(text, 0, 0))) as ComponentRef
}

/// `createPyrunToolDefinition`.
pub fn create_pyrun_tool_definition(evaluate: PyrunEvaluate) -> ToolDefinition {
    let execute = Arc::new(
        move |_tool_call_id: String, params: Value, signal, on_update, ctx: Option<ExtensionContext>| {
            let evaluate = Arc::clone(&evaluate);
            Box::pin(async move {
                let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
                let params = PyrunEvalParams {
                    code: params
                        .get("code")
                        .and_then(Value::as_string)
                        .cloned()
                        .unwrap_or_default(),
                    session_id: params.get("session_id").and_then(Value::as_string).cloned(),
                };
                evaluate(params, ctx, on_update, signal, Some(_tool_call_id)).await
            }) as BoxFuture<'static, Result<AgentToolResult, HandlerError>>
        },
    );
    let prepare_execution: PrepareExecution =
        Arc::new(|_tool_call_id, _signal, _ctx| Box::pin(async { Ok(Some(ToolCallExecution { agent_id: uuidv7() })) }));
    let render_call: ToolRenderer = Arc::new(render_call);
    let render_result: ToolResultRenderer = infallible_render_result(render_result);
    ToolDefinition {
        name: "pyrun_eval".into(),
        label: "Pyrun Eval".into(),
        description: "Evaluate Python/Pyrun code through the canonical Pyrun JSONL runtime adapter.".into(),
        prompt_snippet: Some(PYRUN_PROMPT_SNIPPET.into()),
        prompt_guidelines: Some(PYRUN_PROMPT_GUIDELINES.iter().map(|line| (*line).into()).collect()),
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
        .or_else(|| pi_js_compat::process_env::get("PI_BWRAP_COMMAND").map(JsString::from))
        .unwrap_or_else(|| "bwrap".into());
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
                    agent_id: ctx.tool_execution_agent_id.clone().map(JsString::from),
                    ctx,
                    detach_registry: detach_registry.expect("checked"),
                    dispatch_pi_request: dispatch,
                    on_update,
                    params,
                    pi_bridge_enabled: _pi_bridge,
                    tool_call_id: tool_call_id.map(JsString::from),
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

#[cfg(test)]
mod native_command_tests {
    use super::*;

    #[test]
    fn local_command_names_and_arguments_preserve_unpaired_units() {
        let params = js_json::parse_lossless(r#"{"name":"  command-\ud800  ","args":"arg-\udfff"}"#).unwrap();
        let (name, args) = normalize_command_run_params(Some(&params)).unwrap();
        assert_eq!(name, JsString::from("command-") + JsString::from_utf16(vec![0xd800]));
        assert_eq!(args, JsString::from("arg-") + JsString::from_utf16(vec![0xdfff]));
    }
}

/// Registered factory for `<first-party:pyrun>`.
pub fn pyrun_extension_factory(options: PyrunExtensionOptions) -> ExtensionFactory {
    Arc::new(move |pi| {
        let result = install_pyrun_extension(pi, options.clone());
        Box::pin(async move { result })
    })
}
