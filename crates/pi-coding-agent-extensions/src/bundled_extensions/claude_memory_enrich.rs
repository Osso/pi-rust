//! Bundled `claude-memory-enrich` extension (`extensions/claude-memory-enrich/src/index.ts`).

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use pi_js_compat::abort::{AbortController, AbortSignal};
use pi_js_compat::js_json::{JsString, JsValue as Value};
use pi_js_compat::process_env;
use tokio::io::AsyncWriteExt;

use crate::core::extensions::types::{
    BeforeAgentStartEvent, BeforeAgentStartEventResult, BoxFuture, ExtensionApi, ExtensionContext, HandlerError,
};
use crate::utils::child_process::node_spawn_error_message;
use pi_js_compat::js_json;

const DEFAULT_CLAUDE_MEMORY: &str = "/home/osso/.cargo/bin/claude-memory";
const TIMEOUT_MS: u64 = 75_000;
const KILL_GRACE_MS: u64 = 1_000;
const SECTION_START: &str = "<claude_memory_enrich>";
const SECTION_END: &str = "</claude_memory_enrich>";

#[derive(Default)]
struct EnrichRuntime {
    queue: tokio::sync::Mutex<()>,
    shutdown: AbortController,
}

pub fn claude_memory_enrich_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let runtime = Arc::new(EnrichRuntime::default());
    let shutdown_runtime = runtime.clone();
    pi.on_session_shutdown(move |_, _| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
        let runtime = shutdown_runtime.clone();
        Box::pin(async move {
            runtime.shutdown.abort(None);
            let _queue = runtime.queue.lock().await;
            Ok(None)
        })
    })
    .map_err(HandlerError::new)?;
    pi.on_before_agent_start(
        move |event: BeforeAgentStartEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>> {
            let runtime = runtime.clone();
            Box::pin(async move {
                let prompt = event.prompt.trim().to_owned();
                if prompt.is_empty() {
                    return Ok(None);
                }

                let Some(command) = resolve_claude_memory_command() else {
                    return Ok(None);
                };

                let _queue = runtime.queue.lock().await;
                let shutdown = runtime.shutdown.signal();
                if shutdown.aborted() {
                    return Ok(None);
                }
                match run_enrich(&command, &prompt, ctx.signal()?, shutdown).await {
                    Ok(Some(additional_context)) => Ok(Some(BeforeAgentStartEventResult {
                        message: None,
                        system_prompt: Some(append_enrichment(&event.system_prompt, &additional_context)),
                    })),
                    Ok(None) => Ok(None),
                    Err(error) => {
                        eprintln!("claude-memory-enrich: {error}");
                        Ok(None)
                    }
                }
            })
        },
    )
    .map_err(HandlerError::new)
}

fn resolve_claude_memory_command() -> Option<String> {
    if let Some(configured) = process_env::get("PI_CLAUDE_MEMORY").filter(|value| !value.is_empty()) {
        return Some(configured);
    }
    Path::new(DEFAULT_CLAUDE_MEMORY)
        .exists()
        .then(|| DEFAULT_CLAUDE_MEMORY.to_owned())
}

fn is_hook_output(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let Some(output) = object.get("hookSpecificOutput") else {
        return true;
    };
    let Some(output) = output.as_object() else {
        return false;
    };
    let hook_event_name_valid = output.get("hookEventName").is_none_or(Value::is_string);
    let additional_context_valid = output.get("additionalContext").is_none_or(Value::is_string);
    hook_event_name_valid && additional_context_valid
}

/// `parseAdditionalContext`.
pub fn parse_additional_context(stdout: &str) -> Result<Option<JsString>, HandlerError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let parsed = js_json::parse_lossless(trimmed).map_err(|error| HandlerError::new(error.into_message()))?;
    if !is_hook_output(&parsed) {
        return Err(HandlerError::new("claude-memory enrich returned unexpected JSON shape"));
    }

    let context = parsed
        .get("hookSpecificOutput")
        .and_then(|output| output.get("additionalContext"))
        .and_then(Value::as_string)
        .map(JsString::trim)
        .filter(|context| !context.is_empty());
    Ok(context)
}

async fn run_enrich(
    command: &str,
    prompt: &JsString,
    signal: Option<AbortSignal>,
    shutdown: AbortSignal,
) -> Result<Option<JsString>, HandlerError> {
    if let Some(reason) = signal.as_ref().and_then(AbortSignal::reason) {
        let message = reason
            .as_error()
            .map(|error| error.message)
            .unwrap_or_else(|| "operation aborted".into());
        return Err(HandlerError::new(message));
    }
    let mut child = tokio::process::Command::new(command)
        .arg("enrich")
        .env_clear()
        .envs(process_env::vars())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| HandlerError::new(spawn_error_message(command, &error)))?;

    let pid = child.id().and_then(|id| rustix::process::Pid::from_raw(id as i32));
    let stdin = child.stdin.take();
    let output = async move {
        if let Some(mut stdin) = stdin {
            let payload = format!(
                "{}\n",
                js_json::stringify_lossless(&pi_js_compat::js_json!({"prompt": prompt}))
            );
            stdin.write_all(payload.as_bytes()).await?;
        }
        child.wait_with_output().await
    };
    tokio::pin!(output);
    let termination = tokio::select! {
        result = &mut output => return interpret_output(result.map_err(|error| HandlerError::new(error.to_string()))?),
        () = wait_for_cancellation(signal) => EnrichTermination::Abort,
        _ = shutdown.wait() => EnrichTermination::Shutdown,
        () = tokio::time::sleep(Duration::from_millis(TIMEOUT_MS)) => EnrichTermination::Timeout,
    };
    if let Some(pid) = pid {
        signal_enrich_process(pid, rustix::process::Signal::TERM)?;
        if tokio::time::timeout(Duration::from_millis(KILL_GRACE_MS), &mut output)
            .await
            .is_err()
        {
            signal_enrich_process(pid, rustix::process::Signal::KILL)?;
            output.await.map_err(|error| HandlerError::new(error.to_string()))?;
        }
    }
    match termination {
        EnrichTermination::Shutdown => Ok(None),
        EnrichTermination::Abort => Err(HandlerError::new("operation aborted")),
        EnrichTermination::Timeout => Err(HandlerError::new(format!(
            "claude-memory enrich timed out after {TIMEOUT_MS}ms"
        ))),
    }
}

enum EnrichTermination {
    Abort,
    Shutdown,
    Timeout,
}

fn signal_enrich_process(pid: rustix::process::Pid, signal: rustix::process::Signal) -> Result<(), HandlerError> {
    match rustix::process::kill_process(pid, signal) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(HandlerError::new(format!(
            "claude-memory enrich signal failed: {error}"
        ))),
    }
}

fn interpret_output(output: std::process::Output) -> Result<Option<JsString>, HandlerError> {
    if output.status.code() != Some(0) {
        let code = output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "null".to_owned());
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HandlerError::new(format!(
            "claude-memory enrich exited with {code}: {}",
            stderr.trim()
        )));
    }

    parse_additional_context(&String::from_utf8_lossy(&output.stdout))
}

async fn wait_for_cancellation(signal: Option<AbortSignal>) {
    match signal {
        Some(token) => {
            token.wait().await;
        }
        None => std::future::pending().await,
    }
}

fn append_enrichment(system_prompt: &JsString, additional_context: &JsString) -> JsString {
    if system_prompt.contains(SECTION_START) {
        return system_prompt.to_owned();
    }
    system_prompt.clone() + "\n\n" + SECTION_START + "\n" + additional_context + "\n" + SECTION_END
}

fn spawn_error_message(command: &str, error: &std::io::Error) -> pi_js_compat::js_json::JsString {
    node_spawn_error_message("spawn", &pi_js_compat::js_json::JsString::from(command), error)
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    #[test]
    fn enrichment_json_and_prompt_injection_retain_raw_units() {
        let extra = parse_additional_context(r#"{"hookSpecificOutput":{"additionalContext":"x\ud800"}}"#)
            .unwrap()
            .unwrap();
        let original = JsString::from_utf16(vec![0xdc00]);
        let appended = append_enrichment(&original, &extra);
        assert_eq!(appended.units()[0], 0xdc00);
        assert!(appended.contains(JsString::from_utf16(vec![0x78, 0xd800])));
        let payload = pi_js_compat::js_json!({"prompt":original});
        assert_eq!(
            js_json::parse_lossless(js_json::stringify_lossless(&payload)).unwrap(),
            payload
        );
    }
}
