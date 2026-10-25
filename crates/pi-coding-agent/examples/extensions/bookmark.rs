//! Opt-in native `bookmark.ts`; bookmarks the latest assistant entry, not the leaf.
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry};
use pi_js_compat::{date::js_now_millis, number::js_number_to_string};

pub fn bookmark_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let api = pi.clone();
    pi.register_command(
        "bookmark",
        Some("Bookmark last message (usage: /bookmark [label])".into()),
        None,
        Arc::new(move |args, ctx| {
            let api = api.clone();
            Box::pin(async move {
                let trimmed = (&args).trim();
                let label = if trimmed.is_empty() {
                    pi_js_compat::js_format!("bookmark-{}", js_number_to_string(js_now_millis()))
                } else {
                    trimmed.to_owned()
                };
                let entries = ctx.session_manager()?.get_entries();
                let id = entries.iter().rev().find_map(last_assistant_id);
                let ui = ctx.ui()?;
                if let Some(id) = id {
                    api.set_label(&JsString::from(id), Some(&JsString::from(&label)))?;
                    ui.notify(
                        &JsString::from(pi_js_compat::js_format!("Bookmarked as: {}", label)),
                        Some(NotifyType::Info),
                    )?;
                } else {
                    ui.notify(
                        &JsString::from("No assistant message to bookmark"),
                        Some(NotifyType::Warning),
                    )?;
                }
                Ok(())
            })
        }),
    )?;
    let api = pi.clone();
    pi.register_command(
        "unbookmark",
        Some("Remove bookmark from last labeled entry".into()),
        None,
        Arc::new(move |_args, ctx| {
            let api = api.clone();
            Box::pin(async move {
                let bookmark = find_last_label(&ctx)?;
                let ui = ctx.ui()?;
                if let Some((id, label)) = bookmark {
                    api.set_label(&JsString::from(&id), None)?;
                    ui.notify(
                        &JsString::from(pi_js_compat::js_format!("Removed bookmark: {}", label)),
                        Some(NotifyType::Info),
                    )?;
                } else {
                    ui.notify(&JsString::from("No bookmarked entry found"), Some(NotifyType::Warning))?;
                }
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}

fn last_assistant_id(entry: &SessionEntry) -> Option<&JsString> {
    match entry {
        SessionEntry::Known(KnownSessionEntry::Message(entry)) if entry.message.role() == "assistant" => {
            Some(&entry.id)
        }
        _ => None,
    }
}

fn find_last_label(ctx: &ExtensionContext) -> Result<Option<(JsString, JsString)>, HandlerError> {
    let manager = ctx.session_manager()?;
    Ok(manager.get_entries().iter().rev().find_map(|entry| {
        let id = entry.id()?;
        let label = manager.get_label(id).filter(|label| !label.is_empty())?;
        Some((id.to_owned(), label.to_owned()))
    }))
}
