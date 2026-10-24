//! Port of `core/empty-session-cleanup.ts`.

use std::path::Path;

use pi_ai::request::CallbackError;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;

use super::runtime_process::is_pi_runtime_process_alive;
use super::session_control_db::{
    EmptyMainSessionCandidate, list_empty_main_session_candidates, read_session_health, remove_session_metadata,
};
use super::session_manager::SessionManager;
use crate::utils::paths::node_resolve;

fn is_resident_session(id: &JsString, path: &JsString) -> bool {
    id == "supervisor"
        || id == "architect"
        || path
            .split('/')
            .flat_map(|part| part.split('\\').collect::<Vec<_>>())
            .any(|part| part == "supervisor-sessions" || part == "architect-sessions")
}

fn exists(path: &JsString) -> bool {
    Path::new(&encode_node_utf8(path)).exists()
}

pub fn remove_abandoned_empty_session(
    manager: &SessionManager,
    target_session_file: Option<&JsString>,
) -> Result<(), CallbackError> {
    let Some(db) = manager.get_metadata_control_db_path().filter(|path| !path.is_empty()) else {
        return Ok(());
    };
    let Some(file) = manager.get_session_file().filter(|path| !path.is_empty()) else {
        return Ok(());
    };
    if let Some(target) = target_session_file.filter(|path| !path.is_empty())
        && node_resolve("", target).map_err(CallbackError::from)?
            == node_resolve("", file).map_err(CallbackError::from)?
    {
        return Ok(());
    }
    if is_resident_session(manager.get_session_id(), file)
        || manager
            .get_entries()
            .iter()
            .filter_map(|entry| entry.entry())
            .any(|entry| entry.entry_type() == "message")
    {
        return Ok(());
    }
    remove_session_metadata(db, file).map_err(CallbackError::from)?;
    if exists(file) {
        std::fs::remove_file(encode_node_utf8(file)).map_err(|error| {
            CallbackError::from(NativeNodeError::from(pi_js_compat::node_fs::node_fs_error_record(
                &error, "unlink", file,
            )))
        })?;
    }
    Ok(())
}

fn read_candidate_is_abandoned(db: &JsString, session: &EmptyMainSessionCandidate) -> Result<bool, CallbackError> {
    if is_resident_session(&session.id, &session.session_path) || exists(&session.session_path) {
        return Ok(false);
    }
    let health = read_session_health(db, &session.id).map_err(CallbackError::from)?;
    let pid = health.and_then(|health| health.pid).filter(|pid| *pid != 0);
    Ok(!pid.is_some_and(is_pi_runtime_process_alive))
}

pub fn sweep_abandoned_empty_sessions(db: &JsString) -> Result<usize, CallbackError> {
    let sessions = list_empty_main_session_candidates(db).map_err(CallbackError::from)?;
    let selected = sessions
        .into_iter()
        .map(|session| {
            read_candidate_is_abandoned(db, &session).map(|abandoned| abandoned.then_some(session.session_path))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let abandoned: Vec<_> = selected.into_iter().flatten().collect();
    for path in &abandoned {
        remove_session_metadata(db, path).map_err(CallbackError::from)?;
    }
    Ok(abandoned.len())
}
