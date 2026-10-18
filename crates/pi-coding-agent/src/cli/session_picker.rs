//! Session picker helpers (`cli/session-picker.ts`).
//!
//! `selectSession` needs `SessionSelectorComponent` and `cli/startup-ui.ts`,
//! both still owned by the interactive slices. The session loading/filtering/
//! naming/ordering helper is ported here and is the whole data path behind the
//! selector.

use pi_ai::request::BoxFuture;

use crate::core::session_control_db::list_named_sessions;
use crate::core::session_manager::SessionInfo;

/// `SessionListProgress`, restricted to `Send` callbacks so loader futures stay
/// `Send` (the TypeScript loader is async and crosses awaits).
pub type SessionListProgress<'a> = &'a mut (dyn FnMut(usize, usize) + Send);

/// `SessionsLoader`.
pub trait SessionLoader: Send + Sync {
    fn load<'a>(
        &'a self,
        on_progress: Option<SessionListProgress<'a>>,
    ) -> BoxFuture<'a, Result<Vec<SessionInfo>, String>>;
}

/// `loadSessionsWithControlNames`: filter by archive flag, merge control-db
/// names, and order named sessions first (for the non-archived list) then by
/// modification time descending.
pub async fn load_sessions_with_control_names<'a>(
    loader: &'a dyn SessionLoader,
    control_db_path: Option<&'a str>,
    on_progress: Option<SessionListProgress<'a>>,
    archived: bool,
) -> Result<Vec<SessionInfo>, String> {
    let sessions = loader.load(on_progress).await?;
    let mut filtered_sessions: Vec<SessionInfo> = sessions
        .into_iter()
        .filter(|session| session.is_archived.unwrap_or(false) == archived)
        .collect();
    let Some(control_db_path) = control_db_path else {
        return Ok(filtered_sessions);
    };

    let names = named_sessions_by_path(control_db_path)?;
    for session in &mut filtered_sessions {
        if let Some(name) = names.get(&session.path) {
            session.name = Some(name.clone());
        }
    }
    filtered_sessions.sort_by(|left, right| {
        let left_named = names.contains_key(&left.path);
        let right_named = names.contains_key(&right.path);
        if !archived && left_named != right_named {
            return if left_named {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        right.modified.cmp(&left.modified)
    });
    Ok(filtered_sessions)
}

fn named_sessions_by_path(control_db_path: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let mut names = std::collections::HashMap::new();
    for session in list_named_sessions(control_db_path).map_err(|error| error.to_string())? {
        names.insert(session.session_path, session.name);
    }
    Ok(names)
}
