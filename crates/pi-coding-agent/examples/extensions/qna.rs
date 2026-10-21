//! Opt-in question extraction using the selected model and native bordered loader.
use pi_agent_core::agent::AgentMessage;
use pi_ai::{
    request::StreamOptions,
    types::{
        AssistantContent, Context, Message, StopReason, TextContent, UserContent, UserContentBlock, UserMessage,
        UserRole,
    },
};
use pi_coding_agent::core::{
    extensions::*,
    session_manager::{KnownSessionEntry, SessionEntry},
};
use pi_coding_agent::modes::interactive::components::bordered_loader::{BorderedLoader, BorderedLoaderOptions};
use pi_tui::tui::Component;
use serde_json::Value;
use std::{rc::Rc, sync::Arc};
use tokio_util::sync::CancellationToken;

const SYSTEM_PROMPT: &str = r#"You are a question extractor. Given text from a conversation, extract any questions that need answering and format them for the user to fill in.

Output format:
- List each question on its own line, prefixed with "Q: "
- After each question, add a blank line for the answer prefixed with "A: "
- If no questions are found, output "No questions found in the last message."

Example output:
Q: What is your preferred database?
A: 

Q: Should we use TypeScript or JavaScript?
A: 

Keep questions in the order they appeared. Be concise."#;

pub fn qna_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "qna",
        Some("Extract questions from last assistant message into editor".into()),
        None,
        Arc::new(|_, ctx| {
            Box::pin(async move {
                let ui = ctx.ui()?;
                if ctx.mode()? != ExtensionMode::Tui {
                    ui.notify("qna requires interactive mode", Some(NotifyType::Error));
                    return Ok(());
                }
                let Some(model) = ctx.model()? else {
                    ui.notify("No model selected", Some(NotifyType::Error));
                    return Ok(());
                };
                let branch = ctx.session_manager()?.lock().unwrap().get_branch(None);
                let text = match find_assistant_text(&branch) {
                    AssistantText::Text(text) => text,
                    AssistantText::Incomplete(reason) => {
                        ui.notify(
                            &format!("Last assistant message incomplete ({reason})"),
                            Some(NotifyType::Error),
                        );
                        return Ok(());
                    }
                    AssistantText::Missing => {
                        ui.notify("No assistant messages found", Some(NotifyType::Error));
                        return Ok(());
                    }
                };
                let registry = ctx.model_registry()?;
                let result = ui
                    .custom(
                        Arc::new(move |context, done| {
                            let signal = CancellationToken::new();
                            let loader = QnaLoader {
                                message: format!("Extracting questions using {}...", model.id),
                                signal: signal.clone(),
                                done: done.clone(),
                            };
                            let registry = registry.clone();
                            let model = model.clone();
                            let message = UserMessage {
                                role: UserRole::Tag,
                                content: UserContent::Blocks(vec![UserContentBlock::Text(TextContent::new(
                                    text.clone(),
                                ))]),
                                input_source: None,
                                provider_native: None,
                                timestamp: pi_js_compat::date::js_now_millis() as i64,
                            };
                            tokio::task::spawn_local(async move {
                                let mut options = StreamOptions::default();
                                options.request.signal = Some(signal);
                                let response = registry
                                    .complete(
                                        model,
                                        Context {
                                            system_prompt: Some(SYSTEM_PROMPT.into()),
                                            messages: vec![Message::User(message)],
                                            tools: None,
                                        },
                                        options,
                                    )
                                    .await;
                                let value = match response {
                                    Ok(response) if response.stop_reason != StopReason::Aborted => Value::String(
                                        response
                                            .content
                                            .iter()
                                            .filter_map(|c| match c {
                                                AssistantContent::Text(t) => Some(t.text.as_str()),
                                                _ => None,
                                            })
                                            .collect::<Vec<_>>()
                                            .join("\n"),
                                    ),
                                    Ok(_) | Err(_) => Value::Null,
                                };
                                done.call(Some(value));
                            });
                            context.tui.request_render();
                            Ok(Box::new(loader))
                        }),
                        None,
                    )
                    .await?;
                match result {
                    Some(Value::Null) => ui.notify("Cancelled", Some(NotifyType::Info)),
                    Some(Value::String(text)) => {
                        ui.set_editor_text(&text);
                        ui.notify("Questions loaded. Edit and submit when ready.", Some(NotifyType::Info));
                    }
                    _ => return Err(HandlerError::new("qna custom UI returned neither text nor null")),
                }
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
enum AssistantText {
    Missing,
    Incomplete(String),
    Text(String),
}
fn find_assistant_text(branch: &[SessionEntry]) -> AssistantText {
    for entry in branch.iter().rev() {
        let SessionEntry::Known(KnownSessionEntry::Message(entry)) = entry else {
            continue;
        };
        let AgentMessage::Assistant(message) = entry.message.as_ref() else {
            continue;
        };
        if message.stop_reason != StopReason::Stop {
            let reason = serde_json::to_value(message.stop_reason)
                .expect("stop reason")
                .as_str()
                .expect("string reason")
                .to_owned();
            return AssistantText::Incomplete(reason);
        }
        let parts = message
            .content
            .iter()
            .filter_map(|c| match c {
                AssistantContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if !parts.is_empty() {
            let text = parts.join("\n");
            return if text.is_empty() {
                AssistantText::Missing
            } else {
                AssistantText::Text(text)
            };
        }
    }
    AssistantText::Missing
}
struct QnaLoader {
    message: String,
    signal: CancellationToken,
    done: ExtensionUiDone,
}
impl QnaLoader {
    // BorderedLoader uses Rc. Its explicit empty animation frames make rendering
    // stateless; create/dispose it on the UI thread, retaining only cancellation
    // across the Send+Sync extension boundary. Construction requests one render
    // in the factory; local reconstruction must not request a redraw loop.
    fn native_loader(&self) -> BorderedLoader {
        let mut loader = BorderedLoader::new(Rc::new(|| {}), &self.message, BorderedLoaderOptions::default());
        let signal = self.signal.clone();
        let done = self.done.clone();
        loader.set_on_abort(Some(Rc::new(move || {
            signal.cancel();
            done.call(Some(Value::Null));
        })));
        loader
    }
}
impl ExtensionUiComponent for QnaLoader {
    fn render(&self, width: usize) -> Vec<String> {
        let mut loader = self.native_loader();
        let lines = loader.render(width);
        loader.dispose();
        lines
    }
    fn handle_input(&self, data: &str) {
        let mut loader = self.native_loader();
        loader.handle_input(data);
        loader.dispose();
    }
}
