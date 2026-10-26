//! Opt-in AskClaude tool: native Claude tools, mode restrictions and streamed progress.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use indexmap::IndexMap;
use pi_agent_core::{AgentToolResult, agent::types::AgentToolUpdateCallback};
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{self, JsString, JsValue};
use serde_json::json;

use super::process::{ClaudeProcess, string_prompt_message};
use super::provider::BridgeProvider;
use super::stream::{result_error, string};
use crate::core::extensions::types::{
    ExtensionApi, ExtensionContext, HandlerError, ToolDefinition, require_extension_context,
};

#[derive(Clone)]
pub struct AskDefaults {
    mode: String,
    isolated: bool,
    allow_full: bool,
    append_skills: bool,
}

impl AskDefaults {
    pub fn from_config(config: &JsValue) -> Self {
        let allow_full = config
            .get("allowFullMode")
            .filter(|v| !matches!(v, JsValue::Null))
            .is_none_or(|v| v.as_bool() == Some(true));
        let mode = match config.get("defaultMode").and_then(JsValue::as_string) {
            None => "read",
            Some(v) if v == "read" => "read",
            Some(v) if v == "full" => {
                if allow_full {
                    "full"
                } else {
                    "read"
                }
            }
            _ => "none",
        }
        .to_owned();
        Self {
            mode,
            isolated: config
                .get("defaultIsolated")
                .and_then(JsValue::as_bool)
                .unwrap_or(false),
            allow_full,
            append_skills: config.get("appendSkills").and_then(JsValue::as_bool) != Some(false),
        }
    }

    fn resolve_mode(&self, args: &JsValue) -> Result<String, HandlerError> {
        let mode = args
            .get("mode")
            .map(|v| pi_js_compat::js_json::to_js_string(v).unwrap_or_else(|_| "[object Object]".into()))
            .unwrap_or_else(|| self.mode.as_str().into());
        if mode != "full" && mode != "read" && mode != "none" {
            return Err(HandlerError::new(pi_js_compat::js_format!(
                "Invalid AskClaude mode: {}",
                mode
            )));
        }
        if mode == "full" && !self.allow_full {
            return Err(HandlerError::new("AskClaude full mode is disabled."));
        }
        mode.to_utf8().map_err(|e| HandlerError::new(e.to_string()))
    }

    fn description(&self) -> String {
        let suffix = " Prefer to handle straightforward tasks yourself.";
        let prefix = "Delegate to Claude Code for a second opinion or analysis (code review, architecture questions, debugging theories)";
        if !self.allow_full {
            let middle = if self.mode == "none" {
                "Defaults to no file access — pass mode \"read\" to let Claude Code explore the codebase; it can never make changes."
            } else {
                "Read-only — Claude Code can explore the codebase but not make changes."
            };
            return format!("{prefix}. {middle}{suffix}");
        }
        let middle = match self.mode.as_str() {
            "full" => {
                "Defaults to full mode — Claude Code writes files and runs bash without feedback to pi; pass mode \"read\" to keep it to exploration."
            }
            "none" => {
                "Defaults to no file access — pass mode \"read\" to let Claude Code explore the codebase, or \"full\" for a task that requires changes."
            }
            _ => {
                "Defaults to read-only mode — use full mode when the user wants to delegate a task that requires changes."
            }
        };
        format!("{prefix}, or to autonomously handle a task. {middle}{suffix}")
    }

    fn schema(&self) -> serde_json::Value {
        let mark = |mode: &str| if self.mode == mode { " (default)" } else { "" };
        let mut modes = format!(
            "\"read\"{}: questions about the codebase — review, analysis, explain. \"none\"{}: general knowledge only (no file access).",
            mark("read"),
            mark("none")
        );
        if self.allow_full {
            modes.push_str(&format!(
                " \"full\"{}: allows writing and bash execution (careful: runs without feedback to pi).",
                mark("full")
            ));
        }
        let visibility = if self.isolated {
            "By default Claude sees only this prompt (isolated session)."
        } else {
            "By default Claude sees the full conversation history."
        };
        json!({"type":"object","properties":{
            "prompt":{"type":"string","description":format!("The question or task for Claude Code. {visibility} Don't research up front, let Claude explore.")},
            "mode":{"type":"string","enum":if self.allow_full { vec!["read","full","none"] } else { vec!["read","none"] },"description":modes},
            "model":{"type":"string","description":"Claude model (e.g. \"opus\", \"sonnet\", \"haiku\", or full ID). Defaults to \"opus\"."},
            "thinking":{"type":"string","enum":["off","minimal","low","medium","high","xhigh"],"description":"Thinking effort level. Omit to use Claude Code's default."},
            "isolated":{"type":"boolean","description":format!("When true{}, Claude sees only this prompt (clean session). When false{}, Claude sees the full conversation history.",if self.isolated { " (default)" } else { "" },if self.isolated { "" } else { " (default)" })}
        },"required":["prompt"]})
    }
}

pub fn register(pi: &ExtensionApi, config: &JsValue, bridge: BridgeProvider) -> Result<(), HandlerError> {
    if config.get("enabled").and_then(JsValue::as_bool) != Some(true) {
        return Ok(());
    }
    let defaults = AskDefaults::from_config(config);
    let name = bridge.ask_name.clone();
    let label = config
        .get("label")
        .and_then(JsValue::as_string)
        .cloned()
        .unwrap_or_else(|| "Ask Claude Code".into());
    let description = config
        .get("description")
        .and_then(JsValue::as_string)
        .cloned()
        .unwrap_or_else(|| defaults.description().into());
    let parameters = defaults.schema();
    let render_defaults = defaults.clone();
    pi.register_tool(ToolDefinition {
        name,
        label,
        description,
        parameters,
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(move |_, args, signal, on_update, context| {
            let defaults = defaults.clone();
            let bridge = bridge.clone();
            Box::pin(async move {
                execute(
                    &bridge,
                    &defaults,
                    args,
                    signal,
                    on_update,
                    require_extension_context(context)?,
                )
                .await
            })
        }),
        render_call: Some(Arc::new(move |args, theme, _| {
            Ok(std::rc::Rc::new(std::cell::RefCell::new(
                pi_tui::components::text::Text::new(
                    super::ask_render::call_text(args, &render_defaults.mode, render_defaults.isolated, theme),
                    0,
                    0,
                ),
            )))
        })),
        render_result: Some(Arc::new(|result, options, theme, _| {
            Ok(std::rc::Rc::new(std::cell::RefCell::new(
                pi_tui::components::text::Text::new(
                    super::ask_render::result_text(result, options.expanded, options.is_partial, theme),
                    0,
                    0,
                ),
            )))
        })),
    })
}

async fn execute(
    bridge: &BridgeProvider,
    defaults: &AskDefaults,
    args: JsValue,
    signal: Option<AbortSignal>,
    on_update: Option<AgentToolUpdateCallback>,
    context: ExtensionContext,
) -> Result<AgentToolResult, HandlerError> {
    if context.model()?.is_some_and(|m| m.base_url == "claude-bridge") {
        bridge
            .logs
            .as_ref()
            .map_err(|e| e.clone())?
            .debug("askClaude: blocked circular delegation (active provider is claude-bridge)")?;
        return Ok(tool_result("Error: AskClaude cannot be used when the active provider is claude-bridge — you're already running through Claude Code.".into(),JsValue::object([("error",JsValue::Bool(true))])));
    }
    let mode = defaults.resolve_mode(&args)?;
    let prompt = string(&args, "prompt");
    let start = tokio::time::Instant::now();
    let mut calls = IndexMap::new();
    let cwd = context.cwd()?;
    let isolated = args
        .get("isolated")
        .and_then(JsValue::as_bool)
        .unwrap_or(defaults.isolated);
    let session = context.session_manager()?;
    let history = if isolated {
        Vec::new()
    } else {
        let branch = session.get_branch(None);
        let history = crate::core::session_manager::build_session_context(
            &branch,
            crate::core::session_manager::LeafSelection::Last,
        )
        .messages;
        pi_agent_core::harness::messages::convert_to_llm(&history)
            .map_err(|e| HandlerError::new(e.message().clone()))?
    };
    let result = AskRun {
        bridge,
        history,
        session_id: session.get_session_id(),
        system_prompt: context.get_system_prompt()?,
        append_skills: defaults.append_skills,
        args: &args,
        mode: mode.clone(),
        default_isolated: defaults.isolated,
        signal,
        on_update: on_update.as_ref(),
        calls: &mut calls,
        cwd: &cwd,
        start,
    }
    .run()
    .await;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    match result {
        Ok(mut response) => {
            if let Some(update) = on_update {
                update(tool_result("".into(), JsValue::Object(Default::default())))
                    .map_err(|e| HandlerError::new(e.message))?;
            }
            let actions = action_summary(&calls, &cwd);
            if !actions.is_empty() {
                response.push_str(pi_js_compat::js_format!("\n\n[Claude Code actions: {}]", actions));
            }
            Ok(tool_result(
                response,
                JsValue::object([
                    ("prompt", JsValue::String(prompt)),
                    ("executionTime", JsValue::Number(elapsed)),
                    ("actions", JsValue::String(actions)),
                ]),
            ))
        }
        Err(error) => {
            bridge.logs.as_ref().map_err(|e| e.clone())?.debug_error(
                pi_js_compat::js_format!(
                    "askClaude error: mode={}, model={}, isolated={}, elapsed={}s, error=",
                    mode,
                    args.get("model")
                        .filter(|v| !matches!(v, JsValue::Null))
                        .map(pi_js_compat::js_json::to_js_string)
                        .transpose()?
                        .unwrap_or_else(|| "default".into()),
                    if isolated { "true" } else { "false" },
                    format!("{:.1}", elapsed / 1000.0)
                ),
                &error,
            )?;
            Ok(tool_result(
                pi_js_compat::js_format!("Error: {}", error.message),
                JsValue::object([
                    ("prompt", JsValue::String(prompt)),
                    ("executionTime", JsValue::Number(elapsed)),
                    ("error", JsValue::Bool(true)),
                ]),
            ))
        }
    }
}

struct AskRun<'a> {
    bridge: &'a BridgeProvider,
    history: Vec<pi_ai::types::Message>,
    session_id: JsString,
    system_prompt: JsString,
    append_skills: bool,
    args: &'a JsValue,
    mode: String,
    default_isolated: bool,
    signal: Option<AbortSignal>,
    on_update: Option<&'a AgentToolUpdateCallback>,
    calls: &'a mut IndexMap<JsString, JsValue>,
    cwd: &'a JsString,
    start: tokio::time::Instant,
}

impl AskRun<'_> {
    async fn run(&mut self) -> Result<JsString, CallbackError> {
        let requested = self
            .args
            .get("model")
            .and_then(JsValue::as_string)
            .cloned()
            .unwrap_or_else(|| "opus".into());
        let catalog = super::models::project_models(
            pi_ai::providers::models_data::provider_models("anthropic").unwrap_or_default(),
            &self.bridge.settings,
        );
        let requested = requested.to_utf8().map_err(|e| CallbackError::new(e.to_string()))?;
        let model = super::models::resolve_model(&catalog, &requested)
            .cloned()
            .or_else(|| catalog.first().cloned())
            .ok_or_else(|| CallbackError::new("No Anthropic models available"))?;
        let mut launch = self.bridge.launch_options(
            &model,
            &pi_ai::types::Context {
                system_prompt: None,
                tools: None,
                messages: Vec::new(),
            },
            &Default::default(),
            PathBuf::from(self.cwd.to_utf8().map_err(|e| CallbackError::new(e.to_string()))?),
            "askclaude",
        )?;
        launch.model = super::models::resolve_model(&catalog, &requested)
            .map(|m| super::models::cli_model_id(&m.id, &self.bridge.settings))
            .unwrap_or(requested);
        if self.append_skills && self.mode != "none" {
            launch.append = self
                .bridge
                .captures
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .skills(Some(&self.system_prompt), "native")?;
        }
        launch.ask_mode = Some(self.mode.clone());
        launch.isolated = self
            .args
            .get("isolated")
            .and_then(JsValue::as_bool)
            .unwrap_or(self.default_isolated);
        if !launch.isolated && !self.history.is_empty() {
            let mut context = pi_ai::types::Context {
                system_prompt: None,
                messages: self.history.clone(),
                tools: None,
            };
            context
                .messages
                .push(pi_ai::types::Message::User(pi_ai::types::UserMessage {
                    role: pi_ai::types::UserRole::Tag,
                    content: pi_ai::types::UserContent::Text(string(self.args, "prompt")),
                    input_source: None,
                    provider_native: None,
                    timestamp: super::account_state::now_seconds()? * 1000.0,
                }));
            let mut sessions = self
                .bridge
                .sessions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let store = sessions.as_mut().map_err(|e| e.clone())?;
            store.verification_logs = Some(self.bridge.logs.as_ref().map_err(|e| e.clone())?.clone());
            store.verification_ui = self
                .bridge
                .ui
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            launch.resume = store.sync_ask(&self.session_id, &context, &launch.cwd, &launch.model)?;
        }
        launch.effort = match string(self.args, "thinking")
            .to_utf8()
            .map_err(|e| CallbackError::new(e.to_string()))?
            .as_str()
        {
            "minimal" | "low" => Some("low".into()),
            "medium" => Some("medium".into()),
            "high" => Some("high".into()),
            "xhigh" => Some("max".into()),
            _ => None,
        };
        self.bridge
            .logs
            .as_ref()
            .map_err(|e| e.clone())?
            .debug(pi_js_compat::js_format!(
                "askClaude: mode={} model={} cliModel={} effort={} isolated={} resume={} skills={} promptLen={}",
                self.mode,
                self.args
                    .get("model")
                    .and_then(JsValue::as_string)
                    .cloned()
                    .unwrap_or_else(|| "opus".into()),
                launch.model,
                launch.effort.as_deref().unwrap_or("default"),
                if launch.isolated { "true" } else { "false" },
                launch
                    .resume
                    .as_deref()
                    .map(super::logging::truncate_session_id)
                    .unwrap_or_else(|| "none".into()),
                if launch.append.as_ref().is_some_and(|s| !s.is_empty()) {
                    "true"
                } else {
                    "false"
                },
                string(self.args, "prompt").len()
            ))?;
        let signal = self.signal.clone().unwrap_or_default();
        if signal.aborted() {
            return Err(CallbackError::new("Aborted"));
        }
        let mut process = ClaudeProcess::spawn_child(&launch)?;
        let mut text = JsString::default();
        let mut query_log = (0usize, 0usize, JsString::from("none"));
        let result = tokio::select! {
            biased;
            _ = signal.wait() => None,
            result = self.consume_query(&mut process, &launch, &mut text, &mut query_log) => Some(result),
        };
        let result = match result {
            Some(Ok(())) => Ok(text),
            Some(Err(error)) => {
                if let Some(profile) = launch.cli_debug.as_ref().and_then(|debug| debug.profile.as_ref()) {
                    super::auth_trace::record_failure(profile, &error.message)?;
                }
                process.terminate().await?;
                Err(error)
            }
            None => {
                process.cancel().await?;
                Ok(text)
            }
        };
        if let Ok(text) = &result {
            self.bridge.logs.as_ref().map_err(|e| e.clone())?.debug(pi_js_compat::js_format!("askClaude: done stopReason={} resultSubtype={} sdkMessages={} textDeltas={} responseLen={} toolCalls={}", if signal.aborted() { "cancelled" } else { "stop" }, query_log.2, query_log.0, query_log.1, text.len(), self.calls.len()))?;
        }
        result
    }

    async fn consume_query(
        &mut self,
        process: &mut ClaudeProcess,
        launch: &super::process::LaunchOptions,
        text: &mut JsString,
        query_log: &mut (usize, usize, JsString),
    ) -> Result<(), CallbackError> {
        process.initialize(launch).await?;
        process
            .write(&string_prompt_message(string(self.args, "prompt")))
            .await?;

        let mut interval = tokio::time::interval_at(
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            std::time::Duration::from_secs(1),
        );
        loop {
            tokio::select! {
                _ = interval.tick() => self.update_progress()?,
                frame = process.read() => match frame? {
                    None => break,
                    Some(frame) => {
                        query_log.0 += 1;
                        let event = frame.get("event").unwrap_or(&JsValue::Null);
                        if string(event, "type") == "content_block_delta" && event.get("delta").is_some_and(|d| string(d, "type") == "text_delta") { query_log.1 += 1; }
                        if string(event, "type") == "content_block_start" && let Some(block) = event.get("content_block").filter(|b| string(b, "type") == "tool_use") { self.bridge.logs.as_ref().map_err(|e| e.clone())?.debug(pi_js_compat::js_format!("askClaude: tool_use start: {}", string(block, "name")))?; }
                        if string(&frame,"type") == "result" {
                            query_log.2 = string(&frame, "subtype");
                            if let Some(usage) = frame.get("usage") { self.bridge.logs.as_ref().map_err(|e| e.clone())?.debug(pi_js_compat::js_format!("askClaude: result usage: in={} out={} cacheRead={} cacheWrite={} turns={}", usage.get("input_tokens").map_or_else(|| "undefined".into(), js_json::stringify_lossless), usage.get("output_tokens").map_or_else(|| "undefined".into(), js_json::stringify_lossless), usage.get("cache_read_input_tokens").filter(|v| !matches!(v, JsValue::Null)).map_or_else(|| "0".into(), js_json::stringify_lossless), usage.get("cache_creation_input_tokens").filter(|v| !matches!(v, JsValue::Null)).map_or_else(|| "0".into(), js_json::stringify_lossless), frame.get("num_turns").filter(|v| !matches!(v, JsValue::Null)).map_or_else(|| "?".into(), js_json::stringify_lossless)))?; }
                            if let Some(error) = result_error(&frame) { return Err(CallbackError::new(error)); }
                            if text.is_empty() { *text = string(&frame,"result"); }
                            break;
                        }
                        self.consume_frame(&frame,text);
                    }
                }
            }
        }
        process.close().await
    }

    fn consume_frame(&mut self, frame: &JsValue, text: &mut JsString) {
        if string(frame, "type") == "stream_event" {
            let event = frame.get("event").unwrap_or(&JsValue::Null);
            let delta = event.get("delta").unwrap_or(&JsValue::Null);
            if string(delta, "type") == "text_delta" {
                text.push_str(string(delta, "text"));
            }
            if let Some(block) = event.get("content_block").filter(|v| string(v, "type") == "tool_use") {
                self.calls.insert(string(block, "id"), block.clone());
            }
        } else if string(frame, "type") == "assistant"
            && let Some(blocks) = frame
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(JsValue::as_array)
        {
            for block in blocks.iter().filter(|v| string(v, "type") == "tool_use") {
                self.calls.insert(string(block, "id"), block.clone());
            }
        }
    }

    fn update_progress(&self) -> Result<(), CallbackError> {
        let Some(update) = self.on_update else {
            return Ok(());
        };
        let actions = action_summary(self.calls, self.cwd);
        let summary = if actions.is_empty() {
            "working...".into()
        } else {
            actions
        };
        let elapsed = self.start.elapsed().as_secs_f64();
        let seconds = pi_js_compat::number::to_fixed(elapsed, 0);
        update(tool_result(
            pi_js_compat::js_format!("{}s — {}", seconds, summary),
            JsValue::object([
                ("prompt", JsValue::String(string(self.args, "prompt"))),
                ("executionTime", JsValue::Number(elapsed * 1000.0)),
            ]),
        ))
        .map_err(|e| CallbackError::new(e.message))
    }
}

fn tool_result(text: JsString, details: JsValue) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(details),
        is_error: None,
        terminate: None,
    }
}

fn action_summary(calls: &IndexMap<JsString, JsValue>, cwd: &JsString) -> JsString {
    let mut parts = Vec::new();
    let mut previous = JsString::default();
    for call in calls.values() {
        let name = string(call, "name");
        let verb = name.to_lowercase().split(' ').next().unwrap_or_default();
        let input = call.get("input").unwrap_or(&JsValue::Null);
        let Some(action) = format_action(&verb, &name, input, cwd) else {
            continue;
        };
        if verb == previous {
            if let Some(last) = parts.last_mut() {
                *last = action;
            }
        } else {
            parts.push(action);
        }
        previous = verb;
    }
    JsString::join(parts, "; ")
}

fn format_action(verb: &JsString, name: &JsString, input: &JsValue, cwd: &JsString) -> Option<JsString> {
    let path = ["file_path", "path", "command"].into_iter().find_map(|key| {
        input
            .get(key)
            .and_then(JsValue::as_string)
            .map(|v| if key == "command" { v.slice(0, 80) } else { v.clone() })
    });
    let path = path.map(|path| {
        if verb == "bash" || verb == "terminal" {
            path
        } else {
            short_path(&path, cwd)
        }
    });
    let labels: HashMap<JsString, &str> = [
        ("read", "Read"),
        ("readfile", "Read"),
        ("edit", "Edit"),
        ("write", "Edit"),
        ("writefile", "Edit"),
        ("multiedit", "Edit"),
        ("bash", "Bash"),
        ("terminal", "Bash"),
    ]
    .into_iter()
    .map(|(a, b)| (a.into(), b))
    .collect();
    if let Some(label) = labels.get(verb) {
        return Some(path.map_or_else(|| (*label).into(), |p| pi_js_compat::js_format!("{}({})", label, p)));
    }
    if verb == "askclaude" || verb == "bashoutput" {
        return None;
    }
    for (key, label) in [
        ("glob", "Glob"),
        ("grep", "Grep"),
        ("skill", "Skill"),
        ("agent", "Agent"),
    ] {
        if verb == key {
            let property = if key == "skill" {
                "skill"
            } else if key == "agent" {
                "description"
            } else {
                "pattern"
            };
            let text = string(input, property).slice(0, 40);
            return Some(if text.is_empty() && key != "agent" {
                label.into()
            } else {
                pi_js_compat::js_format!("{}({})", label, text)
            });
        }
    }
    if verb == "todowrite" || verb == "taskcreate" || verb == "taskupdate" {
        let todos = input.get("todos").and_then(JsValue::as_array)?;
        let current = todos
            .iter()
            .find(|v| string(v, "status") == "in_progress")
            .or_else(|| todos.iter().find(|v| string(v, "status") == "pending"))?;
        return Some(string(current, "content").slice(0, 40)).filter(|v| !v.is_empty());
    }
    Some(name.clone())
}

fn short_path(path: &JsString, cwd: &JsString) -> JsString {
    if let Some(relative) = path.strip_prefix(pi_js_compat::js_format!("{}/", cwd)) {
        return relative;
    }
    let parts: Vec<_> = path.split('/').collect();
    if path.starts_with('/') && parts.len() > 3 {
        return JsString::join(&parts[parts.len() - 2..], "/");
    }
    path.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;

    #[test]
    fn claude_bridge_askclaude_actions_collapse_reads_and_keep_shell_commands() {
        let values = [
            r#"{"name":"Read","input":{"file_path":"/project/src/one.rs"}}"#,
            r#"{"name":"Read","input":{"file_path":"/project/src/two.rs"}}"#,
            r#"{"name":"Bash","input":{"command":"/usr/local/bin/tool --version"}}"#,
        ];
        let calls = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| (index.to_string().into(), js_json::parse_lossless(value).unwrap()))
            .collect();
        assert_eq!(
            action_summary(&calls, &"/project".into()),
            "Read(src/two.rs); Bash(/usr/local/bin/tool --version)"
        );
    }

    #[test]
    fn claude_bridge_askclaude_mode_defaults_are_reflected_in_schema() {
        let config =
            js_json::parse_lossless(r#"{"defaultMode":"full","allowFullMode":false,"defaultIsolated":true}"#).unwrap();
        let defaults = AskDefaults::from_config(&config);
        assert_eq!(defaults.mode, "read");
        assert_eq!(defaults.schema()["properties"]["mode"]["enum"], json!(["read", "none"]));
        assert!(
            defaults
                .resolve_mode(&js_json::parse_lossless(r#"{"mode":"full"}"#).unwrap())
                .is_err()
        );
        assert!(defaults.description().contains("Read-only"));
    }
}
