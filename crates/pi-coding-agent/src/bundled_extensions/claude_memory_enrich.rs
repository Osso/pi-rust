//! Bundled `claude-memory-enrich` extension (`extensions/claude-memory-enrich/src/index.ts`).

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use pi_js_compat::process_env;
use tokio::io::AsyncWriteExt;

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
                        system_prompt: Some(append_enrichment(&event.system_prompt, &additional_context).into()),
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
) -> Result<Option<JsString>, HandlerError> {
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
        let payload = format!(
            "{}\n",
            js_json::stringify_lossless(&pi_js_compat::js_json!({"prompt": prompt}))
        );
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

fn spawn_error_message(command: &str, error: &std::io::Error) -> pi_js_compat::js_json::JsString {
    node_spawn_error_message("spawn", &pi_js_compat::js_json::JsString::from(command), error)
}
