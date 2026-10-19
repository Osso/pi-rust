//! Bundled `claude-memory-enrich` extension (`extensions/claude-memory-enrich/src/index.ts`).

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use pi_js_compat::process_env;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::core::extensions::types::{
    BeforeAgentStartEvent, BeforeAgentStartEventResult, BoxFuture, ExtensionApi, ExtensionContext, HandlerError,
};
use crate::utils::child_process::node_spawn_error_message;
use pi_js_compat::js_json;

const DEFAULT_CLAUDE_MEMORY: &str = "/home/osso/.cargo/bin/claude-memory";
const TIMEOUT_MS: u64 = 15_000;
const SECTION_START: &str = "<claude_memory_enrich>";
const SECTION_END: &str = "</claude_memory_enrich>";

pub fn claude_memory_enrich_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_before_agent_start(
        move |event: BeforeAgentStartEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>> {
            Box::pin(async move {
                let prompt = event.prompt.trim().to_owned();
                if prompt.is_empty() {
                    return Ok(None);
                }

                let Some(command) = resolve_claude_memory_command() else {
                    return Ok(None);
                };

                match run_enrich(&command, &prompt, ctx.signal()?).await {
                    Ok(Some(additional_context)) => Ok(Some(BeforeAgentStartEventResult {
                        message: None,
                        system_prompt: Some(append_enrichment(&event.system_prompt, &additional_context)),
                    })),
                    Ok(None) => Ok(None),
                    Err(error) => {
                        eprintln!("claude-memory-enrich: {}", error.message);
                        Ok(None)
                    }
                }
            })
        },
    )
    .map_err(HandlerError::new)
}

fn resolve_claude_memory_command() -> Option<String> {
    if let Some(configured) = process_env::get("PI_CLAUDE_MEMORY") {
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
pub fn parse_additional_context(stdout: &str) -> Result<Option<String>, HandlerError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let parsed = js_json::parse(trimmed).map_err(|error| HandlerError::new(error.to_string()))?;
    if !is_hook_output(&parsed) {
        return Err(HandlerError::new("claude-memory enrich returned unexpected JSON shape"));
    }

    let context = parsed
        .get("hookSpecificOutput")
        .and_then(|output| output.get("additionalContext"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|context| !context.is_empty())
        .map(str::to_owned);
    Ok(context)
}

async fn run_enrich(
    command: &str,
    prompt: &str,
    signal: Option<CancellationToken>,
) -> Result<Option<String>, HandlerError> {
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

    if let Some(stdin) = child.stdin.as_mut() {
        let payload = format!("{}\n", js_json::stringify(&serde_json::json!({"prompt": prompt})));
        let _ = stdin.write_all(payload.as_bytes()).await;
    }
    drop(child.stdin.take());

    enum EnrichOutcome {
        Cancelled,
        TimedOut,
        Finished(Result<crate::utils::child_process::ProcessOutput, std::io::Error>),
    }
    let outcome = tokio::select! {
        () = wait_for_cancellation(signal.clone()) => EnrichOutcome::Cancelled,
        outcome = tokio::time::timeout(
            Duration::from_millis(TIMEOUT_MS),
            crate::utils::child_process::wait_for_child_process(&mut child),
        ) => match outcome {
            Err(_) => EnrichOutcome::TimedOut,
            Ok(result) => EnrichOutcome::Finished(result),
        },
    };

    let output = match outcome {
        EnrichOutcome::Cancelled => {
            let _ = child.start_kill();
            return Err(HandlerError::new("The operation was aborted"));
        }
        EnrichOutcome::TimedOut => {
            let _ = child.start_kill();
            return Err(HandlerError::new(format!(
                "claude-memory enrich timed out after {TIMEOUT_MS}ms"
            )));
        }
        EnrichOutcome::Finished(Err(error)) => return Err(HandlerError::new(error.to_string())),
        EnrichOutcome::Finished(Ok(output)) => output,
    };

    if output.status != Some(0) {
        let code = output
            .status
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

async fn wait_for_cancellation(signal: Option<CancellationToken>) {
    match signal {
        Some(token) => token.cancelled().await,
        None => std::future::pending().await,
    }
}

fn append_enrichment(system_prompt: &str, additional_context: &str) -> String {
    if system_prompt.contains(SECTION_START) {
        return system_prompt.to_owned();
    }
    format!("{system_prompt}\n\n{SECTION_START}\n{additional_context}\n{SECTION_END}")
}

fn spawn_error_message(command: &str, error: &std::io::Error) -> String {
    node_spawn_error_message("spawn", command, error)
}
