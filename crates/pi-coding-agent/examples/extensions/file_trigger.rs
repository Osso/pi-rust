//! Opt-in native `file-trigger.ts`; embed and register explicitly.
use notify::{RecursiveMode, Watcher};
use pi_coding_agent::core::extensions::*;
use pi_js_compat::{node_fs::node_fs_error, string::js_trim};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
const TRIGGER_FILE: &str = "/tmp/agent-trigger.txt";

pub fn file_trigger_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    file_trigger_with_path(pi, trigger_file_path(TRIGGER_FILE))
}

fn trigger_file_path(path: &str) -> PathBuf {
    #[cfg(test)]
    {
        let root = pi_js_compat::process_env::get("PI_TEST_FILE_TRIGGER_ROOT")
            .expect("file-trigger tests must supply a private filesystem root");
        PathBuf::from(root).join(path.trim_start_matches('/'))
    }
    #[cfg(not(test))]
    {
        PathBuf::from(path)
    }
}
/// Hosts/tests may relocate the physical file without changing its protocol text.
pub fn file_trigger_with_path(pi: &ExtensionApi, path: PathBuf) -> Result<(), HandlerError> {
    let api = pi.clone();
    let watchers = Arc::new(Mutex::new(Vec::new()));
    pi.on_session_start(move |_event, ctx: ExtensionContext| {
        let api = api.clone();
        let path = path.clone();
        let watchers = watchers.clone();
        Box::pin(async move {
            let watched = path.clone();
            let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                match event {
                    Ok(event) if event.kind.is_modify() || event.kind.is_remove() => {
                        // Pinned source catches *all* read/send/write failures and leaves the file intact.
                        let _caught = inject_and_clear(&api, &watched);
                    }
                    Ok(_) => {}
                    Err(error) => eprintln!("file-trigger: {error}"),
                }
            })
            .map_err(|error| watch_error(error, &path))?;
            watcher
                .watch(&path, RecursiveMode::NonRecursive)
                .map_err(|error| watch_error(error, &path))?;
            watchers
                .lock()
                .map_err(|error| HandlerError::new(error.to_string()))?
                .push(watcher);
            if ctx.has_ui()? {
                ctx.ui()?
                    .notify(&format!("Watching {TRIGGER_FILE}"), Some(NotifyType::Info));
            }
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })
    .map_err(HandlerError::new)
}
fn watch_error(error: notify::Error, path: &std::path::Path) -> HandlerError {
    let error = match error.kind {
        notify::ErrorKind::Io(error) => error,
        notify::ErrorKind::PathNotFound => std::io::Error::from(std::io::ErrorKind::NotFound),
        _ => return HandlerError::new(error.to_string()),
    };
    HandlerError::new(node_fs_error(&error, "watch", &path.to_string_lossy()))
}
fn inject_and_clear(pi: &ExtensionApi, path: &std::path::Path) -> Result<(), HandlerError> {
    let bytes = std::fs::read(path).map_err(|error| HandlerError::new(error.to_string()))?;
    let text = String::from_utf8_lossy(&bytes);
    let content = js_trim(&text);
    if content.is_empty() {
        return Ok(());
    }
    pi.send_message(
        CustomMessagePayload {
            custom_type: "file-trigger".into(),
            content: CustomMessageContent::Text(format!("External trigger: {content}")),
            display: true,
            details: None,
        },
        Some(SendMessageOptions {
            trigger_turn: Some(true),
            ..Default::default()
        }),
    )?;
    std::fs::write(path, "").map_err(|error| HandlerError::new(error.to_string()))
}
