//! Opt-in context transfer. New-session UI always uses the replacement context.
use pi_agent_core::{
    agent::types::AgentMessage,
    harness::compaction::utils::{serialize_conversation_with_arguments, truncate_for_summary},
};
use pi_ai::{
    request::{ProviderRequestOptions, StreamOptions, now_millis},
    types::{AssistantContent, Context, StopReason},
};
use pi_coding_agent::{
    core::{
        compaction::utils::TOOL_CALL_ARGUMENT_MAX_CHARS, extensions::*, messages::convert_to_llm,
        session_manager::SessionEntry,
    },
    modes::interactive::components::bordered_loader::{BorderedLoader, BorderedLoaderOptions},
};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::JsValue as Value;
use pi_tui::tui::Component;
use std::{rc::Rc, sync::Arc};

const SYSTEM_PROMPT: &str = r#"You are a context transfer assistant. Given a conversation history and the user's goal for a new thread, generate a focused prompt that:

1. Summarizes relevant context from the conversation (decisions made, approaches taken, key findings)
2. Lists any relevant files that were discussed or modified
3. Clearly states the next task based on the user's goal
4. Is self-contained - the new thread should be able to proceed without the old conversation

Format your response as a prompt the user can send to start the new thread. Be concise but include all necessary context. Do not include any preamble like "Here's the prompt" - just output the prompt itself.

Example output format:
## Context
We've been working on X. Key decisions:
- Decision 1
- Decision 2

Files involved:
- path/to/file1.ts
- path/to/file2.ts

## Task
[Clear description of what to do next based on user's goal]"#;
fn entry_message(entry: &SessionEntry) -> Result<Option<AgentMessage>, HandlerError> {
    let value = pi_js_compat::js_json::to_value(&entry).map_err(|e| HandlerError::new(e.to_string()))?;
    let message = match value["type"].as_string() {
        Some(kind) if kind == "message" => Some(value["message"].clone()),
        Some(kind) if kind == "compaction" => Some(
            json!({"role":"compactionSummary","summary":value["summary"],"tokensBefore":value["tokensBefore"],"timestamp":pi_js_compat::date::date_parse_utf16(value["timestamp"].as_string().unwrap().units())}),
        ),
        _ => None,
    };
    // Session JSON objects enumerate numeric keys in JS order before conversation serialization.
    message
        .map(|message| pi_js_compat::js_json::from_value(&message).map_err(|e| HandlerError::new(e.to_string())))
        .transpose()
}
pub(crate) fn handoff_messages(branch: &[SessionEntry]) -> Result<Vec<AgentMessage>, HandlerError> {
    let values = branch
        .iter()
        .map(pi_js_compat::js_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| HandlerError::new(e.to_string()))?;
    let mut ordered = Vec::new();
    if let Some(index) = values.iter().rposition(|entry| entry["type"] == json!("compaction")) {
        ordered.push(&branch[index]);
        if let Some(first) = values
            .iter()
            .position(|entry| entry["id"] == values[index]["firstKeptEntryId"])
            && first < index
        {
            ordered.extend(&branch[first..index]);
        }
        ordered.extend(&branch[index + 1..]);
    } else {
        ordered.extend(branch);
    }
    ordered
        .into_iter()
        .filter_map(|entry| entry_message(entry).transpose())
        .collect()
}
struct Loader {
    done: ExtensionUiDone,
    controller: pi_js_compat::abort::AbortController,
}
impl Loader {
    fn component(&self) -> BorderedLoader {
        // Empty indicator means no timer state; construct Rc UI only on the UI thread.
        let mut loader = BorderedLoader::new(
            Rc::new(|| {}),
            pi_coding_agent::modes::interactive::theme::current_theme(),
            "Generating handoff prompt...",
            BorderedLoaderOptions::default(),
        );
        let done = self.done.clone();
        let controller_signal = self.controller.clone();
        loader.set_on_abort(Some(Rc::new(move || {
            controller_signal.abort(None);
            done.call(Some(Value::Null));
        })));
        loader
    }
}
impl ExtensionUiComponent for Loader {
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        self.component().render(width)
    }
    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        self.component().handle_input(data);
    }
}

pub fn handoff_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "handoff",
        Some("Transfer context to a new focused session".into()),
        None,
        Arc::new(|args, ctx| Box::pin(run_handoff(args, ctx))),
    )
    .map_err(HandlerError::new)
}

async fn run_handoff(args: JsString, ctx: ExtensionCommandContext) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    if ctx.mode()? != ExtensionMode::Tui {
        ui.notify(
            &JsString::from("handoff requires interactive mode"),
            Some(NotifyType::Error),
        )?;
        return Ok(());
    }
    let Some(model) = ctx.model()? else {
        ui.notify(&JsString::from("No model selected"), Some(NotifyType::Error))?;
        return Ok(());
    };
    let goal = args.trim();
    if goal.is_empty() {
        ui.notify(
            &JsString::from("Usage: /handoff <goal for new thread>"),
            Some(NotifyType::Error),
        )?;
        return Ok(());
    }
    let manager = ctx.session_manager()?;
    let (branch, parent) = { (manager.get_branch(None), manager.get_session_file()) };
    let messages = handoff_messages(&branch)?;
    if messages.is_empty() {
        ui.notify(&JsString::from("No conversation to hand off"), Some(NotifyType::Error))?;
        return Ok(());
    }
    let conversation = serialize_conversation_with_arguments(
        &convert_to_llm(&messages).map_err(|error| HandlerError::new(error.message()))?,
        &|value| {
            truncate_for_summary(
                JsString::from(pi_js_compat::js_json::stringify_lossless(value)),
                TOOL_CALL_ARGUMENT_MAX_CHARS,
            )
        },
    );
    let prompt = js_format!(
        "## Conversation History\n\n{}\n\n## User's Goal for New Thread\n\n{}",
        conversation,
        goal
    );
    let draft = generate_draft(&ctx, model, prompt).await?;
    edit_draft_and_replace(&ctx, draft, parent).await
}

async fn generate_draft(
    ctx: &ExtensionCommandContext,
    model: pi_ai::types::Model,
    prompt: JsString,
) -> Result<Option<JsString>, HandlerError> {
    let registry = ctx.model_registry()?;
    let result = ctx
        .ui()?
        .custom(
            Arc::new(move |context, done| {
                let controller_signal = pi_js_compat::abort::AbortController::new();
                let signal = controller_signal.signal();
                tokio::task::spawn_local(complete_draft(
                    registry.clone(),
                    model.clone(),
                    prompt.clone(),
                    signal.clone(),
                    done.clone(),
                ));
                context.tui.request_render();
                pi_coding_agent::core::extensions::ready_custom_component(Box::new(Loader {
                    done,
                    controller: controller_signal,
                }))
            }),
            None,
        )
        .await
        .map_err(HandlerError::from)?;
    Ok(result.and_then(|value| value.as_string().cloned()))
}

async fn complete_draft(
    registry: Arc<pi_coding_agent::core::model_registry::ModelRegistry>,
    model: pi_ai::types::Model,
    prompt: JsString,
    signal: AbortSignal,
    done: ExtensionUiDone,
) {
    let context = Context {
        system_prompt: Some(SYSTEM_PROMPT.into()),
        messages: vec![
            pi_js_compat::js_json::from_value(&json!({
                "role":"user", "content":[{"type":"text","text":prompt}], "timestamp":now_millis()
            }))
            .expect("valid user message"),
        ],
        tools: None,
    };
    let response = async {
        let auth = registry
            .get_api_key_and_headers(&model)
            .await
            .map_err(|error| HandlerError::new(error.message))?;
        let (api_key, headers, env) = match auth {
            pi_coding_agent::core::model_registry::ResolvedRequestAuth::Ok {
                api_key: Some(key),
                headers,
                env,
                ..
            } if !key.is_empty() => (key, headers, env),
            pi_coding_agent::core::model_registry::ResolvedRequestAuth::Ok { .. } => {
                return Err(HandlerError::new(js_format!("No API key for {}", model.provider)));
            }
            pi_coding_agent::core::model_registry::ResolvedRequestAuth::Err { error } => {
                return Err(HandlerError::new(error));
            }
        };
        let options = StreamOptions {
            request: ProviderRequestOptions {
                signal: Some(signal),
                api_key: Some(api_key),
                headers,
                env,
                ..Default::default()
            },
            ..Default::default()
        };
        pi_ai::compat::complete(model, context, options)
            .await
            .map_err(|error| HandlerError::new(error.to_string()))
    }
    .await;
    let result = match response {
        Ok(response) if response.stop_reason != StopReason::Aborted => {
            let text = JsString::join(
                response
                    .content
                    .iter()
                    .filter_map(|content| match content {
                        AssistantContent::Text(text) => Some(text.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                "\n",
            );
            json!(text)
        }
        Ok(_) => Value::Null,
        Err(error) => {
            eprintln!("Handoff generation failed: Error: {error}");
            Value::Null
        }
    };
    done.call(Some(result));
}

async fn edit_draft_and_replace(
    ctx: &ExtensionCommandContext,
    draft: Option<JsString>,
    parent_session: Option<JsString>,
) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    let Some(draft) = draft else {
        ui.notify(&JsString::from("Cancelled"), Some(NotifyType::Info))?;
        return Ok(());
    };
    let Some(edited) = ui
        .editor(&JsString::from("Edit handoff prompt"), Some(&JsString::from(&draft)))
        .await
        .map_err(HandlerError::from)?
    else {
        ui.notify(&JsString::from("Cancelled"), Some(NotifyType::Info))?;
        return Ok(());
    };
    let result = ctx
        .new_session(Some(NewSessionOptions {
            parent_session: parent_session,
            with_session: Some(Arc::new(move |replacement| {
                let edited = edited.clone();
                Box::pin(async move {
                    let ui = replacement.ui()?;
                    ui.set_editor_text(&edited)?;
                    ui.notify(
                        &JsString::from("Handoff ready. Submit when ready."),
                        Some(NotifyType::Info),
                    )?;
                    Ok(())
                })
            })),
            ..Default::default()
        }))
        .await?;
    if result.cancelled {
        ui.notify(&JsString::from("New session cancelled"), Some(NotifyType::Info))?;
    }
    Ok(())
}
