//! Query actors park at Pi tool boundaries while the Claude process remains alive.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pi_ai::request::SimpleStreamOptions;
use pi_ai::types::{AssistantMessageEvent, Context, Model, StopReason, Tool};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, EventSender, create_assistant_message_event_stream};
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{self, JsString, JsValue};
use tokio::sync::mpsc;

use super::process::{ClaudeProcess, LaunchOptions, user_message};
use super::stream::{StreamDecoder, string};

type Sender = EventSender<AssistantMessageEvent, pi_ai::types::AssistantMessage>;

pub enum QueryCommand {
    Resume { sender: Sender, context: Context },
    EndTurn,
}

struct QueryHandle {
    sender: mpsc::UnboundedSender<QueryCommand>,
    tool_ids: Arc<Mutex<HashSet<String>>>,
    session: JsString,
}

#[derive(Default)]
struct QueryState {
    next_id: u64,
    queries: HashMap<u64, QueryHandle>,
}

#[derive(Clone)]
pub struct BridgeProvider {
    state: Arc<Mutex<QueryState>>,
    pub settings: JsValue,
    pub executable: Result<PathBuf, CallbackError>,
    pub ask_name: String,
}

impl BridgeProvider {
    pub fn new(settings: JsValue, executable: Result<PathBuf, CallbackError>, ask_name: String) -> Self {
        Self {
            state: Arc::new(Mutex::new(QueryState::default())),
            settings,
            executable,
            ask_name,
        }
    }

    pub fn stream(
        &self,
        model: Model,
        context: Context,
        options: Option<SimpleStreamOptions>,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        let options = options.unwrap_or_default();
        let (sender, stream) = create_assistant_message_event_stream();
        let results: HashSet<_> = context
            .messages
            .iter()
            .filter_map(|message| match message {
                pi_ai::types::Message::ToolResult(v) => Some(v.tool_call_id.clone()),
                _ => None,
            })
            .collect();
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let owner = state.queries.values().find(|query| {
            query
                .tool_ids
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .any(|id| results.contains(id))
        });
        if let Some(owner) = owner {
            owner
                .sender
                .send(QueryCommand::Resume { sender, context })
                .map_err(|_| CallbackError::new("Claude query ended before tool result delivery"))?;
            return Ok(stream);
        }
        let cwd = options
            .stream
            .cwd
            .as_ref()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| CallbackError::new("claude-bridge: stream options.cwd is required"))?;
        let cwd = PathBuf::from(
            cwd.to_utf8()
                .map_err(|_| CallbackError::new("Invalid Claude working directory"))?,
        );
        state.next_id += 1;
        let id = state.next_id;
        let (commands, receive) = mpsc::unbounded_channel();
        let tool_ids = Arc::new(Mutex::new(HashSet::new()));
        state.queries.insert(
            id,
            QueryHandle {
                sender: commands,
                tool_ids: tool_ids.clone(),
                session: options.stream.session_id.clone().unwrap_or_default(),
            },
        );
        let bridge = self.clone();
        drop(state);
        tokio::spawn(async move {
            let mut actor = QueryActor::new(&model, &context, sender, receive, tool_ids);
            let launch = bridge.launch_options(&model, &context, &options, cwd);
            let result = match launch {
                Ok(launch) => actor.run(&launch, &context, &options).await,
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                actor.fail(error);
            }
            bridge
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .queries
                .remove(&id);
        });
        Ok(stream)
    }

    pub fn end_turn(&self, session: &JsString) {
        for query in self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .queries
            .values()
        {
            if &query.session == session {
                let _closed = query.sender.send(QueryCommand::EndTurn);
            }
        }
    }

    pub fn launch_options(
        &self,
        model: &Model,
        context: &Context,
        options: &SimpleStreamOptions,
        cwd: PathBuf,
    ) -> Result<LaunchOptions, CallbackError> {
        Ok(LaunchOptions {
            executable: self.executable.clone()?,
            cwd,
            model: super::models::cli_model_id(&model.id, &self.settings),
            provider: self.settings.clone(),
            append: context.system_prompt.clone(),
            effort: options.reasoning.map(|v| {
                match v {
                    pi_ai::types::ThinkingLevel::Minimal | pi_ai::types::ThinkingLevel::Low => "low",
                    pi_ai::types::ThinkingLevel::Medium => "medium",
                    pi_ai::types::ThinkingLevel::High => "high",
                    _ => "max",
                }
                .to_owned()
            }),
            resume: None,
            ask_mode: None,
            isolated: false,
            has_tools: context
                .tools
                .as_ref()
                .is_some_and(|v| v.iter().any(|t| t.name != self.ask_name && t.name != "web_search")),
        })
    }
}

struct QueryActor {
    model: Model,
    decoder: StreamDecoder,
    current: Option<Sender>,
    commands: mpsc::UnboundedReceiver<QueryCommand>,
    tool_ids: Arc<Mutex<HashSet<String>>>,
    tools: Vec<Tool>,
    pending_calls: HashMap<String, JsValue>,
    pending_results: HashMap<String, JsValue>,
    turn_ended: bool,
}

impl QueryActor {
    fn new(
        model: &Model,
        context: &Context,
        sender: Sender,
        commands: mpsc::UnboundedReceiver<QueryCommand>,
        tool_ids: Arc<Mutex<HashSet<String>>>,
    ) -> Self {
        let tools = context.tools.clone().unwrap_or_default();
        Self {
            model: model.clone(),
            decoder: StreamDecoder::with_tools(model, tools.iter().map(|t| t.name.clone())),
            current: Some(sender),
            commands,
            tool_ids,
            tools,
            pending_calls: HashMap::new(),
            pending_results: HashMap::new(),
            turn_ended: false,
        }
    }

    async fn run(
        &mut self,
        launch: &LaunchOptions,
        context: &Context,
        options: &SimpleStreamOptions,
    ) -> Result<(), CallbackError> {
        validate_tools(&self.tools)?;
        let mut process = ClaudeProcess::spawn(launch).await?;
        process.write(&user_message(prompt_content(context)?, false)).await?;
        let signal = options.stream.request.signal.clone().unwrap_or_default();
        loop {
            tokio::select! {
                _ = signal.wait() => {
                    process.interrupt().await?;
                    self.decoder.output.stop_reason = StopReason::Aborted;
                    self.decoder.output.error_message = Some("Aborted".into());
                    self.finish();
                    return Ok(());
                }
                command = self.commands.recv() => if let Some(command) = command { self.deliver(command, &mut process).await?; },
                frame = process.read() => match frame? {
                    None => break,
                    Some(frame) => {
                        if string(&frame,"type") == "control_request" { self.control(&frame,&mut process).await?; }
                        else {
                            self.decode(&frame)?;
                            if string(&frame,"type") == "result" { break; }
                        }
                    }
                }
            }
        }
        process.close().await?;
        self.finish();
        Ok(())
    }

    fn decode(&mut self, frame: &JsValue) -> Result<(), CallbackError> {
        if self.current.is_none() {
            return Ok(());
        }
        let events = self.decoder.decode(frame)?;
        for event in events {
            if let AssistantMessageEvent::ToolCallEnd { tool_call, .. } = &event {
                self.tool_ids
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(tool_call.id.clone());
            }
            if let Some(sender) = &self.current {
                sender.push(event);
            }
        }
        if self.decoder.parked {
            self.current.take();
        }
        Ok(())
    }

    fn finish(&mut self) {
        if let Some(sender) = self.current.take() {
            sender.push(self.decoder.finish());
            sender.end(None);
        }
    }

    fn fail(&mut self, error: CallbackError) {
        self.decoder.output.stop_reason = StopReason::Error;
        self.decoder.output.error_message = Some(error.message);
        self.finish();
    }

    async fn deliver(&mut self, command: QueryCommand, process: &mut ClaudeProcess) -> Result<(), CallbackError> {
        match command {
            QueryCommand::EndTurn => self.turn_ended = true,
            QueryCommand::Resume { sender, context } => {
                self.current = Some(sender);
                self.decoder = StreamDecoder::with_tools(&self.model, self.tools.iter().map(|t| t.name.clone()));
                if matches!(context.messages.last(), Some(pi_ai::types::Message::User(_))) {
                    process.write(&user_message(prompt_content(&context)?, true)).await?;
                }
                for message in &context.messages {
                    if let pi_ai::types::Message::ToolResult(result) = message {
                        let value = JsValue::object([
                            (
                                "content",
                                js_json::to_value(&result.content).map_err(|e| CallbackError::new(e.to_string()))?,
                            ),
                            ("isError", JsValue::Bool(result.is_error)),
                        ]);
                        self.pending_results.insert(result.tool_call_id.clone(), value);
                    }
                }
                self.flush_results(process).await?;
            }
        }
        Ok(())
    }

    async fn flush_results(&mut self, process: &mut ClaudeProcess) -> Result<(), CallbackError> {
        let ready: Vec<_> = self
            .pending_calls
            .keys()
            .filter(|id| self.pending_results.contains_key(*id))
            .cloned()
            .collect();
        for id in ready {
            if let (Some(request), Some(result)) = (self.pending_calls.remove(&id), self.pending_results.remove(&id)) {
                self.send_mcp_response(&request, Ok(result), process).await?;
            }
        }
        Ok(())
    }

    async fn control(&mut self, frame: &JsValue, process: &mut ClaudeProcess) -> Result<(), CallbackError> {
        let request = frame.get("request").unwrap_or(&JsValue::Null);
        match string(request, "subtype")
            .to_utf8()
            .map_err(|_| CallbackError::new("Invalid Claude control subtype"))?
            .as_str()
        {
            "mcp_message" => {
                let message = request.get("message").unwrap_or(&JsValue::Null);
                if string(message, "method") == "tools/call" {
                    let params = message.get("params").unwrap_or(&JsValue::Null);
                    let name = string(params, "name");
                    let call_id = params
                        .get("_meta")
                        .and_then(|v| v.get("claudecode/toolUseId"))
                        .and_then(JsValue::as_string);
                    if !self.tools.iter().any(|t| name == t.name.as_str()) {
                        return self
                            .send_mcp_response(frame, Err(pi_js_compat::js_format!("Unknown tool: {}", name)), process)
                            .await;
                    }
                    let Some(call_id) = call_id else {
                        return self.send_mcp_response(frame, Err(pi_js_compat::js_format!("{}: tools/call is missing _meta[\"claudecode/toolUseId\"] — cannot pair the result with its tool call",name)),process).await;
                    };
                    let id = call_id
                        .to_utf8()
                        .map_err(|_| CallbackError::new("Invalid Claude tool-use id"))?;
                    self.pending_calls.insert(id, frame.clone());
                    self.flush_results(process).await?;
                } else {
                    self.send_mcp_response(frame, Ok(mcp_result(message, &self.tools)?), process)
                        .await?;
                }
            }
            "hook_callback" => {
                let result = if self.turn_ended {
                    JsValue::object([("continue", JsValue::Bool(false))])
                } else {
                    JsValue::Object(Default::default())
                };
                process.write(&control_response(frame, result)).await?;
            }
            _ => {
                process
                    .write(&control_response(frame, JsValue::Object(Default::default())))
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_mcp_response(
        &self,
        request: &JsValue,
        result: Result<JsValue, JsString>,
        process: &mut ClaudeProcess,
    ) -> Result<(), CallbackError> {
        let rpc_id = request
            .get("request")
            .and_then(|v| v.get("message"))
            .and_then(|v| v.get("id"))
            .cloned()
            .unwrap_or(JsValue::Number(0.0));
        let response = match result {
            Ok(result) => JsValue::object([
                ("jsonrpc", JsValue::String("2.0".into())),
                ("id", rpc_id),
                ("result", result),
            ]),
            Err(error) => JsValue::object([
                ("jsonrpc", JsValue::String("2.0".into())),
                ("id", rpc_id),
                (
                    "error",
                    JsValue::object([("code", JsValue::Number(-32603.0)), ("message", JsValue::String(error))]),
                ),
            ]),
        };
        process
            .write(&control_response(
                request,
                JsValue::object([("mcp_response", response)]),
            ))
            .await
    }
}

fn control_response(request: &JsValue, response: JsValue) -> JsValue {
    JsValue::object([
        ("type", JsValue::String("control_response".into())),
        (
            "response",
            JsValue::object([
                ("subtype", JsValue::String("success".into())),
                ("request_id", JsValue::String(string(request, "request_id"))),
                ("response", response),
            ]),
        ),
    ])
}

fn mcp_result(message: &JsValue, tools: &[Tool]) -> Result<JsValue, CallbackError> {
    if string(message, "method") == "initialize" {
        return js_json::parse_lossless(r#"{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"custom-tools","version":"1.0.0"}}"#).map_err(|e| CallbackError::new(e.to_string()));
    }
    if string(message, "method") == "tools/list" {
        let tools = tools
            .iter()
            .map(|tool| {
                Ok(JsValue::object([
                    ("name", JsValue::String(tool.name.as_str().into())),
                    ("description", JsValue::String(tool.description.clone())),
                    (
                        "inputSchema",
                        js_json::to_value(&tool.parameters).map_err(|e| CallbackError::new(e.to_string()))?,
                    ),
                ]))
            })
            .collect::<Result<Vec<_>, CallbackError>>()?;
        return Ok(JsValue::object([("tools", JsValue::Array(tools))]));
    }
    Ok(JsValue::Object(Default::default()))
}

fn validate_tools(tools: &[Tool]) -> Result<(), CallbackError> {
    for tool in tools {
        if tool.parameters.get("type").and_then(serde_json::Value::as_str) != Some("object") {
            return Err(CallbackError::new(format!(
                "{}: MCP tool parameters must be an object schema, got {}",
                tool.name, tool.parameters
            )));
        }
    }
    Ok(())
}

pub fn prompt_content(context: &Context) -> Result<JsValue, CallbackError> {
    let Some(pi_ai::types::Message::User(user)) = context.messages.last() else {
        return Ok(JsValue::String("[continue]".into()));
    };
    match &user.content {
        pi_ai::types::UserContent::Text(text) => Ok(JsValue::Array(vec![JsValue::object([
            ("type", JsValue::String("text".into())),
            ("text", JsValue::String(text.clone())),
        ])])),
        pi_ai::types::UserContent::Blocks(blocks) => blocks
            .iter()
            .map(|block| {
                let raw = js_json::to_value(block).map_err(|e| CallbackError::new(e.to_string()))?;
                if string(&raw, "type") == "image" {
                    Ok(JsValue::object([
                        ("type", JsValue::String("image".into())),
                        (
                            "source",
                            JsValue::object([
                                ("type", JsValue::String("base64".into())),
                                ("media_type", JsValue::String(string(&raw, "mimeType"))),
                                ("data", JsValue::String(string(&raw, "data"))),
                            ]),
                        ),
                    ]))
                } else {
                    Ok(raw)
                }
            })
            .collect::<Result<Vec<_>, _>>()
            .map(JsValue::Array),
    }
}

pub fn executable_path(settings: &JsValue, extension_path: &Path) -> Result<PathBuf, CallbackError> {
    if let Some(path) = settings.get("pathToClaudeCodeExecutable").and_then(JsValue::as_string) {
        return Ok(PathBuf::from(
            path.to_utf8()
                .map_err(|_| CallbackError::new("Invalid Claude executable path"))?,
        ));
    }
    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "win32"
    };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    for ancestor in extension_path.ancestors().skip(1) {
        let binary = ancestor.join(format!(
            "node_modules/@anthropic-ai/claude-agent-sdk-{platform}-{arch}/claude"
        ));
        if binary.is_file() {
            return Ok(binary);
        }
    }
    Err(CallbackError::new(
        "Could not find Claude Code executable. Set provider.pathToClaudeCodeExecutable in claude-bridge.json.",
    ))
}
