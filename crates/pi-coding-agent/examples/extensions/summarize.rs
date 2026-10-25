//! Opt-in native summary command; model completion uses the host registry.
use pi_ai::{
    request::{StreamOptions, now_millis},
    types::{AssistantContent, Context},
};
use pi_coding_agent::{
    core::extensions::*,
    modes::interactive::theme::{ThemeColor, get_markdown_theme},
};
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::{self, JsValue as Value};
use pi_tui::{
    Text,
    components::{dynamic_border::DynamicBorder, markdown::Markdown},
    tui::Component,
};
use std::{rc::Rc, sync::Arc};

fn text_parts(content: &Value) -> Vec<&JsString> {
    if let Some(text) = content.as_string() {
        return vec![text];
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|part| part["type"] == json!("text"))
        .filter_map(|part| part["text"].as_string())
        .collect()
}
pub(crate) fn conversation_text(
    entries: &[pi_coding_agent::core::session_manager::SessionEntry],
) -> Result<JsString, HandlerError> {
    let mut sections = Vec::new();
    for entry in entries {
        let entry = js_json::to_value(entry).map_err(|e| HandlerError::new(e.to_string()))?;
        if entry["type"] != json!("message") {
            continue;
        }
        let role = entry["message"]["role"].as_string().cloned().unwrap_or_default();
        if role != "user" && role != "assistant" {
            continue;
        }
        let content = &entry["message"]["content"];
        let mut lines = Vec::new();
        let parts = text_parts(content);
        if !parts.is_empty() {
            let text = JsString::join(parts, "\n").trim();
            if !text.is_empty() {
                lines.push(js_format!(
                    "{}: {}",
                    if role == "user" { "User" } else { "Assistant" },
                    text
                ));
            }
        }
        if role == "assistant" {
            for part in content.as_array().into_iter().flatten() {
                if part["type"] == json!("toolCall")
                    && let Some(name) = part["name"].as_string()
                {
                    let args = part
                        .get("arguments")
                        .filter(|v| !v.is_null())
                        .cloned()
                        .unwrap_or(json!({}));
                    lines.push(js_format!(
                        "Tool {} was called with args {}",
                        name,
                        pi_js_compat::js_json::stringify_lossless(&args)
                    ));
                }
            }
        }
        if !lines.is_empty() {
            sections.push(JsString::join(lines, "\n"));
        }
    }
    Ok(JsString::join(sections, "\n\n"))
}
struct Summary {
    text: JsString,
    context: ExtensionUiFactoryContext,
    done: ExtensionUiDone,
}
impl ExtensionUiComponent for Summary {
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let theme = self.context.theme.clone();
        let border = || {
            let theme = theme.clone();
            DynamicBorder::new(Rc::new(move |s| theme.fg(ThemeColor::Accent, s))).render(width)
        };
        let mut lines = border();
        lines.extend(Text::new(theme.fg(ThemeColor::Accent, theme.bold("Conversation Summary")), 1, 0).render(width));
        lines.extend(Markdown::new(&self.text, 1, 1, get_markdown_theme(), None, Default::default()).render(width));
        lines.extend(Text::new(theme.fg(ThemeColor::Dim, "Press Enter or Esc to close"), 1, 0).render(width));
        lines.extend(border());
        lines
    }
    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        if pi_tui::keys::matches_key(data, "enter") || pi_tui::keys::matches_key(data, "escape") {
            self.done.call(None);
        }
    }
}
pub fn summarize_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "summarize",
        Some("Summarize the current conversation in a custom UI".into()),
        None,
        Arc::new(|_, ctx| Box::pin(run_summary(ctx))),
    )
    .map_err(HandlerError::new)
}

async fn run_summary(ctx: ExtensionCommandContext) -> Result<(), HandlerError> {
    let branch = ctx.session_manager()?.get_branch(None);
    let conversation = conversation_text(&branch)?;
    let has_ui = ctx.has_ui()?;
    let ui = ctx.ui()?;
    if conversation.trim().is_empty() {
        if has_ui {
            ui.notify(&JsString::from("No conversation text found"), Some(NotifyType::Warning))?;
        }
        return Ok(());
    }
    if has_ui {
        ui.notify(&JsString::from("Preparing summary..."), Some(NotifyType::Info))?;
    }
    let registry = ctx.model_registry()?;
    let Some(model) = pi_ai::compat::get_model("openai", "gpt-5.2") else {
        if has_ui {
            ui.notify(
                &JsString::from("Model openai/gpt-5.2 not found"),
                Some(NotifyType::Warning),
            )?;
        }
        return Ok(());
    };
    let request = match registry
        .get_api_key_and_headers(&model)
        .await
        .map_err(|error| HandlerError::new(error.message))?
    {
        pi_coding_agent::core::model_registry::ResolvedRequestAuth::Err { error } => {
            if has_ui {
                ui.notify(&error, Some(NotifyType::Warning))?;
            }
            return Ok(());
        }
        pi_coding_agent::core::model_registry::ResolvedRequestAuth::Ok { api_key, .. }
            if api_key.as_ref().is_none_or(JsString::is_empty) =>
        {
            if has_ui {
                ui.notify(
                    &JsString::from("No API key for openai/gpt-5.2"),
                    Some(NotifyType::Warning),
                )?;
            }
            return Ok(());
        }
        pi_coding_agent::core::model_registry::ResolvedRequestAuth::Ok {
            api_key, headers, env, ..
        } => pi_ai::request::ProviderRequestOptions {
            api_key,
            headers,
            env,
            ..Default::default()
        },
    };
    let text = complete_summary(model, conversation, request).await?;
    if ctx.mode()? == ExtensionMode::Tui {
        ui.custom(
            Arc::new(move |context, done| {
                pi_coding_agent::core::extensions::ready_custom_component(Box::new(Summary {
                    text: text.clone(),
                    context,
                    done,
                }))
            }),
            None,
        )
        .await
        .map_err(HandlerError::from)?;
    }
    Ok(())
}

async fn complete_summary(
    model: pi_ai::types::Model,
    conversation: JsString,
    request: pi_ai::request::ProviderRequestOptions,
) -> Result<JsString, HandlerError> {
    let prompt = js_format!(
        "Summarize this conversation so I can resume it later.\nInclude goals, key decisions, progress, open questions, and next steps.\nKeep it concise and structured with headings.\n\n<conversation>\n{}\n</conversation>",
        conversation
    );
    let context = Context {
        system_prompt: None,
        messages: vec![
            js_json::from_value(&json!({
                "role":"user", "content":[{"type":"text","text":prompt}], "timestamp":now_millis()
            }))
            .expect("valid user message"),
        ],
        tools: None,
    };
    let options = StreamOptions {
        request,
        extra: js_json::from_value(&json!({"reasoningEffort":"high"})).expect("JSON object"),
        ..Default::default()
    };
    let response = pi_ai::compat::complete(model, context, options)
        .await
        .map_err(|error| HandlerError::new(error.to_string()))?;
    Ok(JsString::join(
        response
            .content
            .iter()
            .filter_map(|content| match content {
                AssistantContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        "\n",
    ))
}
