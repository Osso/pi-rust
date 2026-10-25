//! Bundled `claude-bash-hook` extension
//! (`packages/coding-agent/extensions/claude-bash-hook/src/index.ts`).

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use pi_js_compat::string::js_trim;
use pi_js_compat::{
    js_json as json,
    js_json::{self, JsObject as Map, JsString, JsValue as Value},
};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::core::extensions::types::{
    ApprovalReviewerResult, BoxFuture, ExtensionApi, ExtensionContext, HandlerError, ToolCallEvent,
};

const DEFAULT_HOOK_PATH: &str = "/home/osso/.cargo/bin/claude-bash-hook";
const HOOK_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq)]
pub enum ClaudeBashHookReviewResult {
    Allow { updated_input: Option<Map> },
    Ask { reason: JsString },
    Deny { reason: JsString },
    Unavailable,
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeBashHookOptions {
    /// Test seam for `PI_CLAUDE_BASH_HOOK`; `None` follows the source lookup.
    pub command: Option<String>,
    /// Test seam for the fixed 30s timeout.
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HookProcessResult {
    stdout: String,
    stderr: String,
}

fn to_claude_bash_hook_tool_name(tool_name: &str) -> Option<&'static str> {
    match tool_name {
        "bash" => Some("Bash"),
        "pyrun_eval" => Some("pyrun_eval"),
        _ => None,
    }
}

fn has_reviewable_input(tool_name: &str, input: &Map) -> bool {
    if tool_name == "Bash" {
        return input
            .get("command")
            .and_then(Value::as_string)
            .is_some_and(|command| !command.is_empty());
    }
    input
        .get("code")
        .and_then(Value::as_string)
        .is_some_and(|code| !code.is_empty())
}

fn map_hook_output(hook_output: &Map) -> ClaudeBashHookReviewResult {
    let decision = hook_output.get("permissionDecision").and_then(Value::as_string);
    match decision {
        Some(decision) if decision == "allow" => {
            let updated_input = hook_output.get("updatedInput").and_then(Value::as_object).cloned();
            ClaudeBashHookReviewResult::Allow { updated_input }
        }
        Some(decision) if decision == "deny" || decision == "block" => ClaudeBashHookReviewResult::Deny {
            reason: format_hook_reason(hook_output.get("permissionDecisionReason")),
        },
        Some(decision) if decision == "ask" => ClaudeBashHookReviewResult::Ask {
            reason: format_hook_reason(hook_output.get("permissionDecisionReason")),
        },
        _ => ClaudeBashHookReviewResult::Unavailable,
    }
}

fn read_claude_bash_hook_command(options: &ClaudeBashHookOptions) -> Option<String> {
    if let Some(command) = &options.command {
        return Some(command.clone());
    }
    let configured = pi_js_compat::process_env::get("PI_CLAUDE_BASH_HOOK");
    if let Some(configured) = configured.filter(|command| !command.is_empty()) {
        return Some(configured);
    }
    Path::new(DEFAULT_HOOK_PATH)
        .exists()
        .then(|| DEFAULT_HOOK_PATH.to_owned())
}

fn format_hook_reason(reason: Option<&Value>) -> JsString {
    reason
        .and_then(Value::as_string)
        .filter(|reason| !reason.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| "Blocked by claude-bash-hook".into())
}

async fn run_hook_process(command: &str, input: &Value, timeout_ms: u64) -> Result<HookProcessResult, HandlerError> {
    let child = Command::new(command)
        .arg("--harness")
        .arg("codex")
        .env_clear()
        .envs(pi_js_compat::process_env::vars())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(HookProcessResult {
                stdout: String::new(),
                stderr: String::new(),
            });
        }
        Err(error) => {
            return Err(HandlerError::new(
                crate::utils::child_process::node_spawn_error_message("spawn", &command.into(), &error),
            ));
        }
    };
    let pid = child.id().and_then(|id| rustix::process::Pid::from_raw(id as i32));
    let body = js_json::stringify_lossless(input).into_bytes();
    let mut output = Box::pin(async move {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&body).await?;
        }
        child.wait_with_output().await
    });
    let output = match tokio::time::timeout(Duration::from_millis(timeout_ms), &mut output).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Err(HandlerError::new(error.to_string())),
        Err(_) => {
            if let Some(pid) = pid {
                match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                    Err(error) => return Err(HandlerError::new(error.to_string())),
                }
            }
            tokio::spawn(async move {
                if let Err(error) = output.await {
                    eprintln!("claude-bash-hook: {error}");
                }
            });
            return Err(HandlerError::new("The operation was aborted"));
        }
    };
    Ok(HookProcessResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn parse_hook_specific_output(stdout: &str) -> Option<Map> {
    let trimmed = js_trim(stdout);
    if trimmed.is_empty() {
        return None;
    }
    let parsed: Value = js_json::parse_lossless(trimmed).ok()?;
    let object = parsed.as_object()?;
    object.get("hookSpecificOutput").and_then(Value::as_object).cloned()
}

pub async fn review_tool_with_claude_bash_hook(
    event: &ToolCallEvent,
    cwd: &JsString,
    options: &ClaudeBashHookOptions,
) -> Result<ClaudeBashHookReviewResult, HandlerError> {
    let tool_name = event.tool_name.as_str();
    let Some(hook_tool_name) = to_claude_bash_hook_tool_name(tool_name) else {
        return Ok(ClaudeBashHookReviewResult::Unavailable);
    };
    if !has_reviewable_input(hook_tool_name, &event.input) {
        return Ok(ClaudeBashHookReviewResult::Unavailable);
    }
    let Some(hook_command) = read_claude_bash_hook_command(options) else {
        return Ok(ClaudeBashHookReviewResult::Unavailable);
    };

    let mut tool_input = event.input.clone();
    tool_input.insert("cwd".into(), Value::String(cwd.clone()).into());
    let hook_input = json!({
        "access_mode": "supervised",
        "approval_policy": "on-request",
        "cwd": cwd,
        "supports_updated_input": true,
        "tool_input": Value::Object(tool_input),
        "tool_name": hook_tool_name,
    });

    let result = run_hook_process(
        &hook_command,
        &hook_input,
        options.timeout_ms.unwrap_or(HOOK_TIMEOUT_MS),
    )
    .await?;
    let Some(hook_output) = parse_hook_specific_output(&result.stdout) else {
        return Ok(ClaudeBashHookReviewResult::Unavailable);
    };
    Ok(map_hook_output(&hook_output))
}

fn to_reviewer_result(result: ClaudeBashHookReviewResult) -> Option<ApprovalReviewerResult> {
    match result {
        ClaudeBashHookReviewResult::Unavailable => None,
        ClaudeBashHookReviewResult::Allow { updated_input } => Some(ApprovalReviewerResult::Allow { updated_input }),
        ClaudeBashHookReviewResult::Ask { reason } => Some(ApprovalReviewerResult::Ask { reason: Some(reason) }),
        ClaudeBashHookReviewResult::Deny { reason } => Some(ApprovalReviewerResult::Deny { reason }),
    }
}

/// Bundled `claude-bash-hook` extension.
pub fn claude_bash_hook_extension(pi: &ExtensionApi, options: &ClaudeBashHookOptions) -> Result<(), HandlerError> {
    let Some(command) = read_claude_bash_hook_command(options) else {
        return Ok(());
    };
    let available = crate::utils::executable::is_executable_available(&command.into(), &Default::default())
        .map_err(|error| HandlerError::from(*error))?;
    if !available {
        return Ok(());
    }
    let options = options.clone();
    pi.register_approval_reviewer(Arc::new(
        move |event: ToolCallEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<ApprovalReviewerResult>, HandlerError>> {
            let options = options.clone();
            Box::pin(async move {
                let cwd = ctx.cwd()?;
                let result = review_tool_with_claude_bash_hook(&event, &cwd, &options).await?;
                Ok(to_reviewer_result(result))
            })
        },
    ))
    .map_err(HandlerError::new)
}

/// Registers the extension factory under `<first-party:claude-bash-hook>`.
pub fn register(registry: &mut crate::core::extensions::loader::ExtensionRegistry) {
    registry.register(
        "<first-party:claude-bash-hook>",
        Arc::new(|pi| {
            let result = claude_bash_hook_extension(pi, &ClaudeBashHookOptions::default());
            Box::pin(async move { result })
        }),
    );
}
