//! Opt-in native `bookmark.ts`; bookmarks the latest assistant entry, not the leaf.
use std::sync::Arc;

use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry};
use pi_js_compat::{date::js_now_millis, number::js_number_to_string, string::js_trim};

pub fn bookmark_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let api = pi.clone();
    pi.register_command(
        "bookmark",
        Some("Bookmark last message (usage: /bookmark [label])".into()),
        None,
        Arc::new(move |args, ctx| {
            let api = api.clone();
            Box::pin(async move {
                let trimmed = js_trim(&args);
                let label = if trimmed.is_empty() {
                    format!("bookmark-{}", js_number_to_string(js_now_millis()))
                } else {
                    trimmed.to_owned()
                };
                let entries = ctx
                    .session_manager()?
                    .lock()
                    .map_err(|error| HandlerError::new(error.to_string()))?
                    .get_entries();
                let id = entries.iter().rev().find_map(last_assistant_id);
                let ui = ctx.ui()?;
                if let Some(id) = id {
                    api.set_label(id, Some(&label))?;
                    ui.notify(&format!("Bookmarked as: {label}"), Some(NotifyType::Info));
                } else {
                    ui.notify("No assistant message to bookmark", Some(NotifyType::Warning));
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
                    api.set_label(&id, None)?;
                    ui.notify(&format!("Removed bookmark: {label}"), Some(NotifyType::Info));
                } else {
                    ui.notify("No bookmarked entry found", Some(NotifyType::Warning));
                }
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}

fn last_assistant_id(entry: &SessionEntry) -> Option<&str> {
    match entry {
        SessionEntry::Known(KnownSessionEntry::Message(entry)) if entry.message.role() == "assistant" => {
            Some(&entry.id)
        }
        _ => None,
    }
}

fn find_last_label(ctx: &ExtensionContext) -> Result<Option<(String, String)>, HandlerError> {
    let manager = ctx.session_manager()?;
    let manager = manager.lock().map_err(|error| HandlerError::new(error.to_string()))?;
    Ok(manager.get_entries().iter().rev().find_map(|entry| {
        let id = entry.id()?;
        let label = manager.get_label(id).filter(|label| !label.is_empty())?;
        Some((id.to_owned(), label.to_owned()))
    }))
}
