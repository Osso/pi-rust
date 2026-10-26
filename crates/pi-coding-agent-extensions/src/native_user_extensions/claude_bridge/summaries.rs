//! Isolated one-shot summaries never resolve harness captures or mutate session mirrors.
use super::{
    process::{ClaudeProcess, LaunchOptions, user_message},
    provider::{BridgeProvider, quota_rejection},
    stream::{result_error, string},
};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, create_assistant_message_event_stream};
use pi_ai::{
    request::SimpleStreamOptions,
    types::{
        AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DoneReason, ErrorReason, Message, Model,
        StopReason, TextContent,
    },
};
use pi_js_compat::{
    callback_error::CallbackError,
    js_json::{self, JsString, JsValue},
};
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

pub(super) fn stream(
    bridge: BridgeProvider,
    model: Model,
    context: Context,
    options: SimpleStreamOptions,
) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    tokio::spawn(async move {
        let signal = options.stream.request.signal.clone().unwrap_or_default();
        let result = run_summary(&bridge, &model, &context, &options).await;
        let now = match super::account_state::now_seconds() {
            Ok(now) => now * 1000.0,
            Err(error) => {
                sender.fail(pi_ai::utils::event_stream::StreamFailure::new(error.message));
                return;
            }
        };
        let mut output = AssistantMessage::empty(&model.api, &model.provider, &model.id, now);
        match result {
            Ok(text) => {
                output.content.push(AssistantContent::Text(TextContent::new(text)));
                sender.push(AssistantMessageEvent::Done {
                    reason: DoneReason::Stop,
                    message: output,
                });
            }
            Err(error) => {
                let aborted = signal.aborted();
                output.stop_reason = if aborted {
                    StopReason::Aborted
                } else {
                    StopReason::Error
                };
                output.error_message = Some(if aborted {
                    "Operation aborted".into()
                } else {
                    error.message
                });
                sender.push(AssistantMessageEvent::Error {
                    reason: if aborted {
                        ErrorReason::Aborted
                    } else {
                        ErrorReason::Error
                    },
                    error: output,
                });
            }
        }
        sender.end(None);
    });
    stream
}
fn extract_prompt(context: &Context, one_off: bool) -> Result<JsString, CallbackError> {
    if !one_off && (context.messages.len() != 1 || !matches!(context.messages[0], Message::User(_))) {
        let roles = context
            .messages
            .iter()
            .map(|m| match m {
                Message::User(_) => "user",
                Message::Assistant(_) => "assistant",
                Message::ToolResult(_) => "toolResult",
            })
            .collect::<Vec<_>>()
            .join(",");
        return Err(CallbackError::new(format!(
            "isolatedStreamFn: expected exactly 1 user message, got {} ({roles})",
            context.messages.len()
        )));
    }
    let mut users = context
        .messages
        .iter()
        .rev()
        .take_while(|m| matches!(m, Message::User(_)))
        .collect::<Vec<_>>();
    users.reverse();
    let text = users
        .into_iter()
        .map(|m| match m {
            Message::User(user) => js_json::to_value(&user.content)
                .map(|v| super::convert::content_text(&v))
                .map_err(|e| CallbackError::new(e.to_string())),
            _ => Err(CallbackError::new("Expected summary user message")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let prompt = JsString::join(text.into_iter().filter(|t| !t.is_empty()), "\n");
    if prompt.is_empty() {
        return Err(CallbackError::new(if one_off {
            "runIsolatedSummary: one-off summary without a user prompt (last message is not user?)"
        } else {
            "isolatedStreamFn: summarization prompt is empty"
        }));
    }
    Ok(prompt)
}
struct Attempt {
    text: JsString,
    error: Option<JsString>,
    quota: Option<Option<f64>>,
}
async fn consume(
    launch: &LaunchOptions,
    prompt: &JsString,
    signal: &pi_js_compat::abort::AbortSignal,
) -> Result<Attempt, CallbackError> {
    if signal.aborted() {
        return Err(CallbackError::new("Operation aborted"));
    }
    let mut process = ClaudeProcess::spawn_child(launch)?;
    let result = tokio::select! {
        biased;
        _ = signal.wait() => None,
        result = consume_query(&mut process, launch, prompt) => Some(result),
    };
    match result {
        Some(Ok(attempt)) => Ok(attempt),
        Some(Err(error)) => {
            process.terminate().await?;
            Err(error)
        }
        None => {
            process.cancel().await?;
            Err(CallbackError::new("Operation aborted"))
        }
    }
}

async fn consume_query(
    process: &mut ClaudeProcess,
    launch: &LaunchOptions,
    prompt: &JsString,
) -> Result<Attempt, CallbackError> {
    process.initialize(launch).await?;
    process
        .write(&user_message(JsValue::String(prompt.clone()), false))
        .await?;
    let mut assistant = JsString::default();
    let mut final_text = JsString::default();
    let mut error = None;
    let mut quota = None;
    let mut rejection = None;
    while let Some(frame) = process.read().await? {
        match string(&frame, "type")
            .to_utf8()
            .map_err(|e| CallbackError::new(e.to_string()))?
            .as_str()
        {
            "assistant" => {
                if let Some(content) = frame
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(JsValue::as_array)
                {
                    for block in content {
                        if string(block, "type") == "text" {
                            assistant.push_str(string(block, "text"));
                        }
                    }
                }
            }
            "rate_limit_event" => {
                if let Some(info) = frame.get("rate_limit_info")
                    && string(info, "status") == "rejected"
                {
                    rejection = Some(info.clone());
                }
            }
            "result" => {
                error = result_error(&frame);
                quota = error
                    .as_ref()
                    .and_then(|error| quota_rejection(error, rejection.as_ref()));
                if let (Some(failure), Some(rejection)) = (&error, &rejection) {
                    error = Some(super::rate_limit::label(rejection, failure));
                }
                if error.is_none() {
                    final_text = string(&frame, "result");
                }
                break;
            }
            _ => {}
        }
    }
    process.close().await?;
    Ok(Attempt {
        text: if final_text.is_empty() { assistant } else { final_text },
        error,
        quota,
    })
}
async fn run_summary(
    bridge: &BridgeProvider,
    model: &Model,
    context: &Context,
    options: &SimpleStreamOptions,
) -> Result<JsString, CallbackError> {
    let prompt = extract_prompt(
        context,
        options.stream.cache_retention == Some(pi_ai::types::CacheRetention::None),
    )?;
    let cwd = options
        .stream
        .cwd
        .as_ref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CallbackError::new("claude-bridge: stream options.cwd is required"))?
        .to_utf8()
        .map(PathBuf::from)
        .map_err(|e| CallbackError::new(e.to_string()))?;
    let mut unprojected = context.clone();
    unprojected.system_prompt = None;
    unprojected.tools = None;
    let signal = options.stream.request.signal.clone().unwrap_or_default();
    let mut pending: Option<VecDeque<String>> = None;
    let mut launch = match bridge.launch_options(model, &unprojected, options, cwd.clone(), "compact-summary") {
        Err(e) if e.message == super::account_state::ACCOUNTS_EXHAUSTED => {
            let profiles = bridge.recheck_accounts(&model.id, &signal).await?;
            pending = (!profiles.is_empty()).then(|| profiles.into());
            let name = pending.as_mut().and_then(VecDeque::pop_front);
            bridge.launch_options_for_profile(
                model,
                &unprojected,
                options,
                cwd.clone(),
                name.as_deref(),
                "compact-summary",
            )?
        }
        other => other?,
    };
    let mut attempted = HashSet::new();
    loop {
        launch.summary = true;
        launch.summary_system_prompt = context.system_prompt.clone();
        launch.append = None;
        launch.isolated = true;
        launch.effort = None;
        launch.has_tools = false;
        if let Some(name) = &launch.account_profile
            && !attempted.insert(name.clone())
        {
            return Err(CallbackError::new(super::account_state::ACCOUNTS_EXHAUSTED));
        }
        let checked_at = (bridge.usage_runtime.now)() / 1000.0;
        let attempt = consume(&launch, &prompt, &signal).await?;
        if let Some(debug) = &launch.cli_debug
            && let Some(profile) = &debug.profile
        {
            super::auth_trace::record(profile, "settled", Some("isolated"))?;
            if let Some(error) = &attempt.error {
                super::auth_trace::record_failure(profile, error)?;
            }
        }
        if let (Some(error), Some(reset), Some(name)) = (&attempt.error, attempt.quota, &launch.account_profile) {
            let next = {
                let mut accounts = bridge
                    .accounts
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                accounts
                    .as_mut()
                    .map_err(|e| e.clone())?
                    .as_mut()
                    .ok_or_else(|| CallbackError::new(error.clone()))?
                    .reject(name, reset, super::account_state::now_seconds()?)?
            };
            let next = if let Some(pending) = &mut pending {
                pending.pop_front()
            } else {
                next
            };
            let next = next.ok_or_else(|| CallbackError::new(super::account_state::ACCOUNTS_EXHAUSTED))?;
            launch = bridge.launch_options_for_profile(
                model,
                &unprojected,
                options,
                cwd.clone(),
                Some(&next),
                "compact-summary",
            )?;
            continue;
        }
        if let Some(error) = attempt.error {
            return Err(CallbackError::new(error));
        }
        if pending.is_some()
            && let Some(name) = &launch.account_profile
        {
            let mut accounts = bridge
                .accounts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(manager) = accounts.as_mut().map_err(|e| e.clone())? {
                manager.recover(name, checked_at)?;
            }
        }
        if attempt.text.trim().is_empty() {
            return Err(CallbackError::new("Claude Code summary returned empty text"));
        }
        return Ok(attempt.text);
    }
}
