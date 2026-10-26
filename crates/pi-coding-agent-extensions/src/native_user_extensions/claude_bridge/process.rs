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
    pub cwd: PathBuf,
    pub model: String,
    pub provider: JsValue,
    pub append: Option<JsString>,
    pub effort: Option<String>,
    pub resume: Option<String>,
    pub claude_dir: Option<PathBuf>,
    pub account_profile: Option<String>,
    pub ask_mode: Option<String>,
    pub isolated: bool,
    pub has_tools: bool,
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
        if let Some(effort) = &self.effort {
            args.extend(["--effort".into(), effort.clone()]);
        }
        if let Some(resume) = &self.resume {
            args.push(format!("--resume={resume}"));
        }
        if let Some(mode) = &self.ask_mode {
            args.extend(["--disallowedTools".into(), disallowed_tools(mode).join(",")]);
            args.push("--setting-sources=user,project".into());
        } else {
            args.extend(["--tools".into(), "WebSearch".into()]);
            if self.has_tools {
                args.extend([
                    "--mcp-config".into(),
                    r#"{"mcpServers":{"custom-tools":{"type":"sdk","name":"custom-tools"}}}"#.into(),
                ]);
            }
        }
        args.extend(["--permission-mode".into(), "bypassPermissions".into()]);
        if self.ask_mode.is_none() {
            args.push("--include-partial-messages".into());
        }
        if self.isolated {
            args.push("--no-session-persistence".into());
        }
        args.extend(["--model".into(), self.model.clone()]);
        if self.ask_mode.is_some() || self.provider.get("strictMcpConfig").and_then(JsValue::as_bool) != Some(false) {
            args.push("--strict-mcp-config".into());
        }
        if self.effort.is_some() {
            args.extend(["--thinking-display".into(), "summarized".into()]);
        }
        args.extend(["--settings".into(), js_json::stringify_lossless(&self.settings())]);
        Ok(args)
    }

    fn settings(&self) -> JsValue {
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
        if self.has_tools {
            request.push((
                "hooks",
                js_json::parse_lossless(r#"{"PostToolBatch":[{"hookCallbackIds":["hook_0"]}]}"#)
                    .expect("static hook JSON"),
            ));
            request.push((
                "sdkMcpServers",
                JsValue::Array(vec![JsValue::String("custom-tools".into())]),
            ));
        }
        if let Some(append) = &self.append {
            request.push(("appendSystemPrompt", JsValue::String(append.clone())));
        }
        if self.ask_mode.is_some() {
            request.push(("skills", JsValue::Array(Vec::new())));
        }
        JsValue::object([
            ("type", JsValue::String("control_request".into())),
            ("request_id", JsValue::String("1".into())),
            ("request", JsValue::object(request)),
        ])
    }
}

pub struct ClaudeProcess {
    child: Child,
    input: ChildStdin,
    output: tokio::io::Lines<BufReader<ChildStdout>>,
    stderr: tokio::task::JoinHandle<Result<String, std::io::Error>>,
}

impl ClaudeProcess {
    pub async fn spawn(options: &LaunchOptions) -> Result<Self, CallbackError> {
        let mut command = Command::new(&options.executable);
        command
            .args(options.arguments()?)
            .current_dir(&options.cwd)
            .env("CLAUDE_CODE_ENTRYPOINT", "sdk-ts")
            .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
            .env("ENABLE_CLAUDEAI_MCP_SERVERS", "0")
            .env("DISABLE_AUTO_COMPACT", "1")
            .env_remove("NODE_OPTIONS")
            .env_remove("DEBUG")
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
        let stderr = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut text = String::new();
            stderr.read_to_string(&mut text).await?;
            Ok(text)
        });
        let mut process = Self {
            child,
            input,
            output: BufReader::new(output).lines(),
            stderr,
        };
        process.write(&options.initialize()).await?;
        loop {
            let frame = process
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
        Ok(process)
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

    pub async fn close(mut self) -> Result<(), CallbackError> {
        self.input
            .shutdown()
            .await
            .map_err(|e| CallbackError::new(e.to_string()))?;
        let status = self.child.wait().await.map_err(|e| CallbackError::new(e.to_string()))?;
        let stderr = self
            .stderr
            .await
            .map_err(|e| CallbackError::new(e.to_string()))?
            .map_err(|e| CallbackError::new(e.to_string()))?;
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
