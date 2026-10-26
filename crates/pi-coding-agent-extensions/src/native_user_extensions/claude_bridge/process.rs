//! The subset of the Agent SDK subprocess protocol used by the bridge.
use std::path::PathBuf;
use std::process::Stdio;

use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{self, JsString, JsValue};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

pub const CLAUDE_MD_EXCLUDES: [&str; 3] = ["**/CLAUDE.md", "**/AGENTS.md", "**/.claude/rules/**"];

pub struct LaunchOptions {
    pub executable: PathBuf,
    pub cli_debug: Option<super::logging::CliDebug>,
    pub cwd: PathBuf,
    pub model: String,
    pub provider: JsValue,
    pub append: Option<JsString>,
    pub summary_system_prompt: Option<JsString>,
    pub summary: bool,
    pub effort: Option<String>,
    pub resume: Option<String>,
    pub preserve_shared_session: bool,
    pub claude_dir: Option<PathBuf>,
    pub account_profile: Option<String>,
    pub account_request_started_at: Option<f64>,
    pub account_retries_remaining: bool,
    pub ask_mode: Option<String>,
    pub isolated: bool,
    pub mcp_manifest: Option<JsValue>,
}

impl LaunchOptions {
    pub fn arguments(&self) -> Result<Vec<String>, CallbackError> {
        let mut args = [
            "--output-format",
            "stream-json",
            "--verbose",
            "--input-format",
            "stream-json",
        ]
        .map(str::to_owned)
        .to_vec();
        self.append_sdk_arguments(&mut args)?;
        self.append_extra_arguments(&mut args);
        args.extend(["--settings".into(), js_json::stringify_lossless(&self.settings())]);
        Ok(args)
    }

    fn append_sdk_arguments(&self, args: &mut Vec<String>) -> Result<(), CallbackError> {
        if let Some(effort) = &self.effort {
            args.extend(["--effort".into(), effort.clone()]);
        }
        if self.summary {
            args.extend(["--max-turns".into(), "1".into(), "--model".into(), self.model.clone()]);
        }
        self.append_debug_arguments(args)?;
        if let Some(resume) = &self.resume {
            args.push(format!("--resume={resume}"));
        }
        self.append_tool_arguments(args);
        if !self.summary {
            args.extend(["--permission-mode".into(), "bypassPermissions".into()]);
        }
        if self.ask_mode.is_none() && !self.summary {
            args.push("--include-partial-messages".into());
        }
        if self.isolated {
            args.push("--no-session-persistence".into());
        }
        Ok(())
    }

    fn append_debug_arguments(&self, args: &mut Vec<String>) -> Result<(), CallbackError> {
        if let Some(path) = self.cli_debug.as_ref().and_then(|debug| debug.debug_file.as_ref()) {
            let path = path
                .to_str()
                .ok_or_else(|| CallbackError::new("Invalid Claude debug path"))?;
            args.extend(["--debug-file".into(), path.into()]);
        }
        Ok(())
    }

    fn append_tool_arguments(&self, args: &mut Vec<String>) {
        if self.summary {
            args.extend([
                "--tools".into(),
                "".into(),
                "--setting-sources=".into(),
                "--strict-mcp-config".into(),
            ]);
        } else if let Some(mode) = &self.ask_mode {
            args.extend(["--disallowedTools".into(), disallowed_tools(mode).join(",")]);
            args.push("--setting-sources=user,project".into());
        } else {
            args.extend(["--tools".into(), "WebSearch".into()]);
        }
    }

    fn append_extra_arguments(&self, args: &mut Vec<String>) {
        if self.summary {
            return;
        }
        // AskClaude inserts strict before model; the provider inserts model first.
        if self.ask_mode.is_some() {
            args.push("--strict-mcp-config".into());
        }
        if self.model.len() > 1 && self.model.starts_with('-') {
            args.push(format!("--model={}", self.model));
        } else {
            args.extend(["--model".into(), self.model.clone()]);
        }
        if self.ask_mode.is_none() && self.provider.get("strictMcpConfig").and_then(JsValue::as_bool) != Some(false) {
            args.push("--strict-mcp-config".into());
        }
        if self.effort.is_some() {
            args.extend(["--thinking-display".into(), "summarized".into()]);
        }
    }

    fn settings(&self) -> JsValue {
        if self.summary {
            return JsValue::object([("autoMemoryEnabled", JsValue::Bool(false))]);
        }
        let mut fields = vec![
            (
                "autoMemoryEnabled",
                JsValue::Bool(
                    self.provider
                        .get("autoMemoryEnabled")
                        .and_then(JsValue::as_bool)
                        .unwrap_or(false),
                ),
            ),
            (
                "claudeMdExcludes",
                JsValue::Array(CLAUDE_MD_EXCLUDES.map(|v| JsValue::String(v.into())).to_vec()),
            ),
        ];
        if self.ask_mode.is_none() {
            fields.push(("includeGitInstructions", JsValue::Bool(false)));
        }
        JsValue::object(fields)
    }

    fn initialize(&self) -> JsValue {
        let mut request = vec![("subtype", JsValue::String("initialize".into()))];
        if !self.summary && self.ask_mode.is_none() {
            request.push((
                "hooks",
                js_json::parse_lossless(r#"{"PostToolBatch":[{"hookCallbackIds":["hook_0"]}]}"#)
                    .expect("static hook JSON"),
            ));
            if let Some(manifest) = &self.mcp_manifest {
                request.push((
                    "sdkMcpServers",
                    JsValue::Array(vec![JsValue::String("custom-tools".into())]),
                ));
                request.push((
                    "sdkMcpServerManifests",
                    JsValue::object([("custom-tools", manifest.clone())]),
                ));
            }
        }
        if let Some(append) = &self.append {
            request.push(("appendSystemPrompt", JsValue::String(append.clone())));
        }
        if self.summary {
            let system = self.summary_system_prompt.clone().unwrap_or_default();
            request.push(("systemPrompt", JsValue::Array(vec![JsValue::String(system)])));
        }
        if self.ask_mode.is_some() || self.summary {
            request.push(("skills", JsValue::Array(Vec::new())));
        }
        JsValue::object([
            ("request_id", JsValue::String("1".into())),
            ("type", JsValue::String("control_request".into())),
            ("request", JsValue::object(request)),
        ])
    }
}

// SDK fT and ProcessTransport.initialize: preserve caller values except Node/debug plumbing.
/// Bridge activation sets this on its own process.env (index.ts:2891), so every
/// Claude Code child it spawns inherits it.
pub const DISABLE_NONESSENTIAL_TRAFFIC: (&str, &str) = ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");

fn sdk_environment(
    mut env: std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
) -> std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString> {
    for (key, value) in [
        ("CLAUDE_CODE_ENTRYPOINT", "sdk-ts"),
        ("CLAUDE_AGENT_SDK_VERSION", "0.3.293"),
    ] {
        if env.get(std::ffi::OsStr::new(key)).is_none_or(|value| value.is_empty()) {
            env.insert(key.into(), value.into());
        }
    }
    if !env.keys().any(|key| {
        key.as_encoded_bytes()
            .eq_ignore_ascii_case(b"CLAUDE_CODE_SDK_READS_SESSION_STATE")
    }) {
        env.insert("CLAUDE_CODE_SDK_READS_SESSION_STATE".into(), "1".into());
    }
    env.remove(std::ffi::OsStr::new("NODE_OPTIONS"));
    env.remove(std::ffi::OsStr::new("DEBUG"));
    let debug = env
        .get(std::ffi::OsStr::new("DEBUG_CLAUDE_AGENT_SDK"))
        .and_then(|value| value.to_str());
    if debug.is_some_and(|value| ["1", "true", "yes", "on"].contains(&value.trim().to_ascii_lowercase().as_str())) {
        env.insert("DEBUG".into(), "1".into());
    }
    env
}

pub struct ClaudeProcess {
    child: Child,
    input: ChildStdin,
    output: tokio::io::Lines<BufReader<ChildStdout>>,
    stderr: Option<tokio::task::JoinHandle<Result<String, std::io::Error>>>,
}

impl ClaudeProcess {
    pub fn spawn_child(options: &LaunchOptions) -> Result<Self, CallbackError> {
        let script = [b".js".as_slice(), b".mjs", b".tsx", b".ts", b".jsx"]
            .iter()
            .any(|suffix| options.executable.as_os_str().as_encoded_bytes().ends_with(suffix));
        let mut command = if script {
            let mut command = Command::new("node");
            command.arg(&options.executable);
            command
        } else {
            Command::new(&options.executable)
        };
        command
            .args(options.arguments()?)
            .current_dir(&options.cwd)
            .env_clear()
            .envs(sdk_environment(std::env::vars_os().collect()))
            .env(DISABLE_NONESSENTIAL_TRAFFIC.0, DISABLE_NONESSENTIAL_TRAFFIC.1)
            .env("ENABLE_CLAUDEAI_MCP_SERVERS", "0")
            .env("DISABLE_AUTO_COMPACT", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(dir) = &options.claude_dir {
            command.env("CLAUDE_CONFIG_DIR", dir);
        }
        let mut child = command
            .spawn()
            .map_err(|e| CallbackError::new(format!("Failed to spawn Claude Code: {e}")))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| CallbackError::new("Claude stdin unavailable"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| CallbackError::new("Claude stdout unavailable"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| CallbackError::new("Claude stderr unavailable"))?;
        let debug = options.cli_debug.clone();
        let stderr = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut text = String::new();
            let mut decoder = pi_js_compat::text_decoder::Utf8StreamDecoder::new();
            let mut bytes = [0; 4096];
            loop {
                let count = stderr.read(&mut bytes).await?;
                let chunk = if count == 0 {
                    decoder.finish()
                } else {
                    decoder.decode(&bytes[..count])
                };
                if let Some(debug) = &debug {
                    debug
                        .stderr(&chunk)
                        .map_err(|e| std::io::Error::other(pi_js_compat::js_json::encode_node_utf8(&e.message)))?;
                }
                text.push_str(&chunk);
                if count == 0 {
                    break;
                }
            }
            Ok(text)
        });
        let process = Self {
            child,
            input,
            output: BufReader::new(output).lines(),
            stderr: Some(stderr),
        };
        Ok(process)
    }

    pub async fn initialize(&mut self, options: &LaunchOptions) -> Result<(), CallbackError> {
        self.write(&options.initialize()).await?;
        loop {
            let frame = self
                .read()
                .await?
                .ok_or_else(|| CallbackError::new("Claude Code closed before initialize"))?;
            if super::stream::string(&frame, "type") == "control_response" {
                let response = frame.get("response").unwrap_or(&JsValue::Null);
                if super::stream::string(response, "subtype") == "error" {
                    return Err(CallbackError::new(super::stream::string(response, "error")));
                }
                break;
            }
        }
        Ok(())
    }

    pub async fn write(&mut self, frame: &JsValue) -> Result<(), CallbackError> {
        let mut line = js_json::stringify_lossless(frame);
        line.push('\n');
        self.input
            .write_all(line.as_bytes())
            .await
            .map_err(|e| CallbackError::new(format!("Claude Code stdin: {e}")))?;
        self.input
            .flush()
            .await
            .map_err(|e| CallbackError::new(format!("Claude Code stdin: {e}")))
    }

    pub async fn read(&mut self) -> Result<Option<JsValue>, CallbackError> {
        self.output
            .next_line()
            .await
            .map_err(|e| CallbackError::new(format!("Claude Code stdout: {e}")))?
            .map(|line| js_json::parse_lossless(line.as_str()).map_err(|e| CallbackError::new(e.to_string())))
            .transpose()
    }

    pub async fn close(&mut self) -> Result<(), CallbackError> {
        self.input
            .shutdown()
            .await
            .map_err(|e| CallbackError::new(e.to_string()))?;
        let status = self.child.wait().await.map_err(|e| CallbackError::new(e.to_string()))?;
        let stderr = self.read_stderr().await?;
        if !status.success() {
            return Err(CallbackError::new(format!(
                "Claude Code process exited with code {}{}",
                status.code().unwrap_or(1),
                if stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {stderr}")
                }
            )));
        }
        Ok(())
    }

    pub async fn terminate(&mut self) -> Result<(), CallbackError> {
        self.child.kill().await.map_err(|e| CallbackError::new(e.to_string()))?;
        if self.stderr.is_some() {
            self.read_stderr().await?;
        }
        Ok(())
    }

    async fn read_stderr(&mut self) -> Result<String, CallbackError> {
        let task = self
            .stderr
            .as_mut()
            .ok_or_else(|| CallbackError::new("Claude stderr already consumed"))?;
        let result = task.await;
        self.stderr = None;
        result
            .map_err(|e| CallbackError::new(e.to_string()))?
            .map_err(|e| CallbackError::new(e.to_string()))
    }

    pub async fn cancel(&mut self) -> Result<(), CallbackError> {
        // The SDK starts interrupt without awaiting it, then closes the query.
        // Do not let a full stdin pipe prevent that close from reaping the child.
        tokio::select! {
            biased;
            _ = self.interrupt() => {},
            _ = std::future::ready(()) => {},
        }
        self.terminate().await
    }

    pub async fn interrupt(&mut self) -> Result<(), CallbackError> {
        self.write(
            &js_json::parse_lossless(
                r#"{"type":"control_request","request_id":"interrupt","request":{"subtype":"interrupt"}}"#,
            )
            .expect("static interrupt JSON"),
        )
        .await
    }
}

/// SDK `query()` with a string prompt writes this record (sdk.mjs:142 `mT`).
pub fn string_prompt_message(text: JsString) -> JsValue {
    JsValue::object([
        ("type", JsValue::String("user".into())),
        ("session_id", JsValue::String("".into())),
        (
            "message",
            JsValue::object([
                ("role", JsValue::String("user".into())),
                (
                    "content",
                    JsValue::Array(vec![JsValue::object([
                        ("type", JsValue::String("text".into())),
                        ("text", JsValue::String(text)),
                    ])]),
                ),
            ]),
        ),
        ("parent_tool_use_id", JsValue::Null),
    ])
}

pub fn user_message(content: JsValue, steer: bool) -> JsValue {
    let mut fields = vec![
        ("type", JsValue::String("user".into())),
        (
            "message",
            JsValue::object([("role", JsValue::String("user".into())), ("content", content)]),
        ),
        ("parent_tool_use_id", JsValue::Null),
    ];
    if steer {
        fields.push(("priority", JsValue::String("next".into())));
    }
    JsValue::object(fields)
}

fn disallowed_tools(mode: &str) -> Vec<&'static str> {
    let mut tools = vec![
        "AskUserQuestion",
        "EnterPlanMode",
        "ExitPlanMode",
        "ToolSearch",
        "ScheduleWakeup",
    ];
    match mode {
        "read" => tools.extend([
            "Write",
            "Edit",
            "Bash",
            "NotebookEdit",
            "EnterWorktree",
            "ExitWorktree",
            "CronCreate",
            "CronDelete",
            "TeamCreate",
            "TeamDelete",
        ]),
        "none" => tools.extend([
            "Read",
            "Write",
            "Edit",
            "Glob",
            "Grep",
            "Bash",
            "Agent",
            "NotebookEdit",
            "EnterWorktree",
            "ExitWorktree",
            "CronCreate",
            "CronDelete",
            "TeamCreate",
            "TeamDelete",
            "WebFetch",
            "WebSearch",
        ]),
        _ => {}
    }
    tools
}
