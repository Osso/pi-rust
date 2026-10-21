//! Opt-in context transfer. New-session UI always uses the replacement context.
use pi_agent_core::{
    agent::types::AgentMessage,
    harness::compaction::utils::{serialize_conversation_with_arguments, truncate_for_summary},
};
use pi_ai::{
    request::{ProviderRequestOptions, StreamOptions, now_millis},
    types::{AssistantContent, CacheRetention, Context, StopReason},
    utils::uuid::uuidv7,
};
use pi_coding_agent::{
    core::{
        compaction::utils::TOOL_CALL_ARGUMENT_MAX_CHARS, extensions::*, messages::convert_to_llm,
        session_manager::SessionEntry,
    },
    modes::interactive::components::bordered_loader::{BorderedLoader, BorderedLoaderOptions},
};
use pi_js_compat::string::js_trim;
use pi_tui::tui::Component;
use serde_json::{Value, json};
use std::{rc::Rc, sync::Arc};
use tokio_util::sync::CancellationToken;

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
    let value = serde_json::to_value(entry).map_err(|e| HandlerError::new(e.to_string()))?;
    let message = match value["type"].as_str() {
        Some("message") => Some(value["message"].clone()),
        Some("compaction") => Some(
            json!({"role":"compactionSummary","summary":value["summary"],"tokensBefore":value["tokensBefore"],"timestamp":pi_js_compat::date::date_parse(value["timestamp"].as_str().unwrap())}),
        ),
        _ => None,
    };
    // Session JSON objects enumerate numeric keys in JS order before conversation serialization.
    message
        .map(|message| {
            serde_json::from_str(&pi_js_compat::js_json::stringify(&message))
                .map_err(|e| HandlerError::new(e.to_string()))
        })
        .transpose()
}
fn handoff_messages(branch: &[SessionEntry]) -> Result<Vec<AgentMessage>, HandlerError> {
    let values = branch
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| HandlerError::new(e.to_string()))?;
    let mut ordered = Vec::new();
    if let Some(index) = values.iter().rposition(|entry| entry["type"] == "compaction") {
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
    signal: CancellationToken,
}
impl Loader {
    fn component(&self) -> BorderedLoader {
        // Empty indicator means no timer state; construct Rc UI only on the UI thread.
        let mut loader = BorderedLoader::new(
            Rc::new(|| {}),
            "Generating handoff prompt...",
            BorderedLoaderOptions::default(),
        );
        let done = self.done.clone();
        let signal = self.signal.clone();
        loader.set_on_abort(Some(Rc::new(move || {
            signal.cancel();
            done.call(Some(Value::Null));
        })));
        loader
    }
}
impl ExtensionUiComponent for Loader {
    fn render(&self, width: usize) -> Vec<String> {
        self.component().render(width)
    }
    fn handle_input(&self, data: &str) {
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

async fn run_handoff(args: String, ctx: ExtensionCommandContext) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    if ctx.mode()? != ExtensionMode::Tui {
        ui.notify("handoff requires interactive mode", Some(NotifyType::Error));
        return Ok(());
    }
    let Some(model) = ctx.model()? else {
        ui.notify("No model selected", Some(NotifyType::Error));
        return Ok(());
    };
    let goal = js_trim(&args);
    if goal.is_empty() {
        ui.notify("Usage: /handoff <goal for new thread>", Some(NotifyType::Error));
        return Ok(());
    }
    let manager = ctx.session_manager()?;
    let (branch, parent) = {
        let manager = manager.lock().map_err(|error| HandlerError::new(error.to_string()))?;
        (manager.get_branch(None), manager.get_session_file().map(str::to_owned))
    };
    let messages = handoff_messages(&branch)?;
    if messages.is_empty() {
        ui.notify("No conversation to hand off", Some(NotifyType::Error));
        return Ok(());
    }
    let conversation = serialize_conversation_with_arguments(&convert_to_llm(&messages), &|value| {
        truncate_for_summary(&pi_js_compat::js_json::stringify(value), TOOL_CALL_ARGUMENT_MAX_CHARS)
    });
    let prompt = format!("## Conversation History\n\n{conversation}\n\n## User's Goal for New Thread\n\n{goal}");
    let draft = generate_draft(&ctx, model, prompt).await?;
    edit_draft_and_replace(&ctx, draft, parent).await
}

async fn generate_draft(
    ctx: &ExtensionCommandContext,
    model: pi_ai::types::Model,
    prompt: String,
) -> Result<Option<String>, HandlerError> {
    let registry = ctx.model_registry()?;
    let result = ctx
        .ui()?
        .custom(
            Arc::new(move |context, done| {
                let signal = CancellationToken::new();
                tokio::task::spawn_local(complete_draft(
                    registry.clone(),
                    model.clone(),
                    prompt.clone(),
                    signal.clone(),
                    done.clone(),
                ));
                context.tui.request_render();
                pi_coding_agent::core::extensions::ready_custom_component(Box::new(Loader { done, signal }))
            }),
            None,
        )
        .await
        .map_err(HandlerError::new)?;
    Ok(result.and_then(|value| value.as_str().map(str::to_owned)))
}

async fn complete_draft(
    registry: Arc<pi_coding_agent::core::model_registry::ModelRegistry>,
    model: pi_ai::types::Model,
    prompt: String,
    signal: CancellationToken,
    done: ExtensionUiDone,
) {
    let context = Context {
        system_prompt: Some(SYSTEM_PROMPT.into()),
        messages: vec![
            serde_json::from_value(json!({
                "role":"user", "content":[{"type":"text","text":prompt}], "timestamp":now_millis()
            }))
            .expect("valid user message"),
        ],
        tools: None,
    };
    let options = StreamOptions {
        request: ProviderRequestOptions {
            signal: Some(signal),
            ..Default::default()
        },
        cache_retention: Some(CacheRetention::None),
        session_id: Some(uuidv7()),
        ..Default::default()
    };
    let result = match registry.complete(model, context, options).await {
        Ok(response) if response.stop_reason != StopReason::Aborted => {
            let text = response
                .content
                .iter()
                .filter_map(|content| match content {
                    AssistantContent::Text(text) => Some(text.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
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
    draft: Option<String>,
    parent_session: Option<String>,
) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    let Some(draft) = draft else {
        ui.notify("Cancelled", Some(NotifyType::Info));
        return Ok(());
    };
    let Some(edited) = ui
        .editor("Edit handoff prompt", Some(&draft))
        .await
        .map_err(HandlerError::new)?
    else {
        ui.notify("Cancelled", Some(NotifyType::Info));
        return Ok(());
    };
    let result = ctx
        .new_session(Some(NewSessionOptions {
            parent_session,
            with_session: Some(Arc::new(move |replacement| {
                let edited = edited.clone();
                Box::pin(async move {
                    let ui = replacement.ui()?;
                    ui.set_editor_text(&edited);
                    ui.notify("Handoff ready. Submit when ready.", Some(NotifyType::Info));
                    Ok(())
                })
            })),
            ..Default::default()
        }))
        .await?;
    if result.cancelled {
        ui.notify("New session cancelled", Some(NotifyType::Info));
    }
    Ok(())
}
