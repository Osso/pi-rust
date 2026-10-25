//! Frozen `extensions/session-archive/src/index.ts`: destructive work runs only at quit.
use std::sync::{Arc, Mutex};

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;

use crate::core::extensions::types::{
    ExtensionApi, ExtensionCommandContext, HandlerError, NotifyType, SessionShutdownEvent, SessionShutdownReason,
};
use crate::core::session_archive_storage::archive_persisted_session;
use crate::core::session_control_db::{read_session_metadata, unarchive_session};
use crate::core::session_delete::delete_session_tree;

struct CurrentSession {
    db: JsString,
    path: JsString,
}

enum QuitAction {
    Archive(CurrentSession),
    Delete(CurrentSession),
}

fn read_current_session_and_notify(
    command: &str,
    args: &pi_js_compat::js_json::JsString,
    ctx: &ExtensionCommandContext,
) -> Result<Option<CurrentSession>, HandlerError> {
    if !args.trim().is_empty() {
        ctx.ui()?
            .notify(&format!("Usage: /{command}").into(), Some(NotifyType::Warning))?;
        return Ok(None);
    }
    let Some(db) = ctx.control_db_path()?.filter(|path| !path.is_empty()) else {
        ctx.ui()?.notify(
            &format!("Session {command} requires a control database.").into(),
            Some(NotifyType::Error),
        )?;
        return Ok(None);
    };
    let path = ctx.session_manager()?.get_session_file();
    let Some(path) = path.filter(|path| !path.is_empty()) else {
        ctx.ui()?.notify(
            &"The current session is not persisted.".into(),
            Some(NotifyType::Warning),
        )?;
        return Ok(None);
    };
    Ok(Some(CurrentSession { db, path }))
}

fn unarchive_current_session(session: CurrentSession, ctx: &ExtensionCommandContext) -> Result<(), HandlerError> {
    let archived = read_session_metadata(&session.db, &session.path)
        .map_err(|error| HandlerError::new(error.message().clone()))?
        .is_some_and(|row| row.is_archived == Some(true));
    if !archived {
        ctx.ui()?
            .notify(&"Current session is already unarchived.".into(), Some(NotifyType::Info))?;
        return Ok(());
    }
    unarchive_session(&session.db, &session.path).map_err(|error| HandlerError::new(error.message().clone()))?;
    ctx.ui()?
        .notify(&"Unarchived current session.".into(), Some(NotifyType::Info))?;
    Ok(())
}

fn count_writing_children(ctx: &ExtensionCommandContext) -> Result<usize, HandlerError> {
    Ok(ctx.multi_agent_store()?.map_or(0, |store| {
        store
            .list_active_agents()
            .iter()
            .filter(|agent| {
                agent
                    .transcript
                    .as_ref()
                    .and_then(|transcript| transcript.path.as_ref())
                    .is_some_and(|path| !path.is_empty())
            })
            .count()
    }))
}

async fn confirm_delete(ctx: &ExtensionCommandContext) -> Result<bool, HandlerError> {
    let count = count_writing_children(ctx)?;
    if count > 0 {
        let (agents, subject, transcripts) = if count == 1 {
            ("agent", "it", "its transcript")
        } else {
            ("agents", "they", "their transcripts")
        };
        ctx.ui()?.notify(
            &format!(
                "Close the {count} running child {agents} before /delete; {subject} could still write {transcripts}."
            )
            .into(),
            Some(NotifyType::Warning),
        )?;
        return Ok(false);
    }
    ctx.ui()?
        .confirm(
            &"Delete session?".into(),
            &"Delete the current session and its child agent sessions, then quit.".into(),
            None,
        )
        .await
        .map_err(HandlerError::from)
}

async fn run_quit_action(action: QuitAction) -> Result<(), JsString> {
    match action {
        QuitAction::Archive(session) => archive_persisted_session(&session.db, &session.path)
            .map(|_| ())
            .map_err(|error| js_format!("Failed to archive session: {}", error.message())),
        QuitAction::Delete(session) => match delete_session_tree(&session.path, Some(&session.db)).await {
            crate::core::session_delete::SessionDeleteResult::Success { .. } => Ok(()),
            crate::core::session_delete::SessionDeleteResult::Failure { error, .. } => {
                Err(JsString::from("Failed to delete session: ") + error)
            }
        },
    }
}

type PendingQuitAction = Arc<Mutex<Option<QuitAction>>>;

fn register_quit_handler(pi: &ExtensionApi, pending: PendingQuitAction) -> Result<(), HandlerError> {
    pi.on_session_shutdown(move |event: SessionShutdownEvent, _ctx| {
        let action = pending.lock().expect("quit action lock").take();
        Box::pin(async move {
            if event.reason != SessionShutdownReason::Quit {
                return Ok(None);
            }
            let Some(action) = action else {
                return Ok(None);
            };
            if let Err(error) = run_quit_action(action).await {
                eprintln!("{}", pi_js_compat::js_json::encode_node_utf8(&error));
            }
            Ok(None)
        }) as crate::core::extensions::types::BoxFuture<'static, Result<Option<()>, HandlerError>>
    })
    .map_err(HandlerError::new)
}

async fn execute_session_command(
    command: &str,
    args: &pi_js_compat::js_json::JsString,
    ctx: ExtensionCommandContext,
    pending: PendingQuitAction,
) -> Result<(), HandlerError> {
    let Some(session) = read_current_session_and_notify(command, args, &ctx)? else {
        return Ok(());
    };
    if command == "unarchive" {
        return unarchive_current_session(session, &ctx);
    }
    if command == "delete" && !confirm_delete(&ctx).await? {
        return Ok(());
    }
    let action = if command == "archive" {
        QuitAction::Archive(session)
    } else {
        QuitAction::Delete(session)
    };
    *pending.lock().expect("quit action lock") = Some(action);
    ctx.shutdown()
}

pub fn session_archive_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let pending = Arc::new(Mutex::new(None));
    register_quit_handler(pi, Arc::clone(&pending))?;
    for (command, description) in [
        ("unarchive", "Unarchive the current session"),
        ("archive", "Archive the current session and quit"),
        (
            "delete",
            "Delete the current session and its child agent sessions, then quit",
        ),
    ] {
        let pending = Arc::clone(&pending);
        pi.register_command(
            command,
            Some(description.into()),
            None,
            Arc::new(move |args, ctx| {
                let pending = Arc::clone(&pending);
                Box::pin(async move { execute_session_command(command, &args, ctx, pending).await })
            }),
        )
        .map_err(HandlerError::new)?;
    }
    Ok(())
}
