//! Opt-in native `auto-commit-on-exit.ts`; stages every change in the host cwd.
//! Enable deliberately: shutdown commits may include unrelated work.
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry};
use pi_js_compat::js_json::JsString;
use pi_js_compat::string::{js_trim, utf16_len, utf16_slice_to_utf8};

pub fn auto_commit_on_exit_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let api = pi.clone();
    pi.on_session_shutdown(
        move |_: SessionShutdownEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let api = api.clone();
            Box::pin(async move {
                let status = api
                    .exec("git".into(), vec!["status".into(), "--porcelain".into()], None)
                    .await?;
                if status.code != 0 || js_trim(&status.stdout).is_empty() {
                    return Ok(None);
                }
                let entries = ctx.session_manager()?.get_entries();
                let text = last_assistant_text(&entries)?;
                let first = text
                    .split('\n')
                    .next()
                    .filter(|line| !line.is_empty())
                    .unwrap_or("Work in progress");
                let length = utf16_len(first);
                // String-typed UI boundary replaces a split surrogate, matching the
                // terminal and Node's git argv encoding (compatibility.md §Out of scope).
                let commit = format!(
                    "[pi] {}{}",
                    utf16_slice_to_utf8(first, 0, length.min(50)),
                    if length > 50 { "..." } else { "" }
                );
                api.exec("git".into(), vec!["add".into(), "-A".into()], None).await?;
                let result = api
                    .exec("git".into(), vec!["commit".into(), "-m".into(), commit.clone()], None)
                    .await?;
                if result.code == 0 && ctx.has_ui()? {
                    ctx.ui()?.notify(
                        &JsString::from(format!("Auto-committed: {commit}")),
                        Some(NotifyType::Info),
                    )?;
                }
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}

fn last_assistant_text(entries: &[SessionEntry]) -> Result<String, HandlerError> {
    let assistant = entries.iter().rev().find_map(|entry| match entry {
        SessionEntry::Known(KnownSessionEntry::Message(entry)) if entry.message.role() == "assistant" => {
            Some(&entry.message)
        }
        _ => None,
    });
    let Some(message) = assistant else {
        return Ok(String::new());
    };
    let value = serde_json::to_value(message).map_err(|e| HandlerError::new(e.to_string()))?;
    Ok(value["content"]
        .as_array()
        .map(|content| {
            content
                .iter()
                .filter(|part| part["type"] == "text")
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default())
}
