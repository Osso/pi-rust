//! Port of `packages/coding-agent/extensions/goal/src/goal-state.ts`.

use std::path::Path;

use pi_js_compat::js_json::{self, JsString, encode_node_utf8};
use pi_js_compat::string::encode_uri_component;

use crate::core::extensions::types::HandlerError;
use crate::core::readonly_session_manager::ReadonlySessionManager;
use crate::utils::paths::node_join;

use super::goal_parsing::{parse_goal, parse_goal_json};
use super::goal_types::Goal;

/// `encodeURIComponent` throws `URIError` for an unpaired surrogate; it propagates like TS.
fn goal_path_for_session_id(cwd: &JsString, session_id: &JsString) -> Result<JsString, HandlerError> {
    let encoded = encode_uri_component(session_id).map_err(|error| HandlerError::new(error.to_string()))?;
    let goals_dir = node_join(node_join(cwd.clone(), ".pi"), "goals");
    Ok(node_join(goals_dir, format!("{encoded}.json")))
}

fn goal_path(cwd: &JsString, session: &ReadonlySessionManager) -> Result<JsString, HandlerError> {
    goal_path_for_session_id(cwd, &session.get_session_id())
}

fn old_project_goal_path(cwd: &JsString) -> JsString {
    node_join(node_join(cwd.clone(), ".pi"), "goal.json")
}

fn file_exists(file: &JsString) -> bool {
    Path::new(&encode_node_utf8(file)).exists()
}

fn remove_file(file: &JsString) -> Result<(), HandlerError> {
    std::fs::remove_file(encode_node_utf8(file)).map_err(|error| HandlerError::new(error.to_string()))
}

fn save_goal_json(session: &ReadonlySessionManager, goal: &Goal) -> Result<(), HandlerError> {
    let serialized = js_json::to_value(goal).map_err(|error| HandlerError::new(error.to_string()))?;
    let mut text = JsString::from(js_json::stringify_lossless(&serialized));
    text.push_str("\n");
    session
        .set_session_goal_json(&text)
        .map_err(|error| HandlerError::new(error.message().clone()))
}

/// TS wraps read + parse in `try { ... } catch { return null; }`, so every failure is `None`.
fn load_goal_file(file: &JsString) -> Option<Goal> {
    if !file_exists(file) {
        return None;
    }
    let bytes = std::fs::read(encode_node_utf8(file)).ok()?;
    let value = js_json::parse_lossless(String::from_utf8_lossy(&bytes).as_ref()).ok()?;
    parse_goal(&value)
}

/// `loadOrMigrateGoal`.
pub fn load_or_migrate_goal(cwd: &JsString, session: &ReadonlySessionManager) -> Result<Option<Goal>, HandlerError> {
    if let Some(stored_goal) = session
        .get_session_goal_json()
        .map_err(|error| HandlerError::new(error.message().clone()))?
        .filter(|stored_goal| !stored_goal.is_empty())
    {
        return Ok(parse_goal_json(&stored_goal));
    }
    migrate_legacy_goal(cwd, session)
}

/// `loadOrMigrateActiveGoal`.
pub fn load_or_migrate_active_goal(
    cwd: &JsString,
    session: &ReadonlySessionManager,
) -> Result<Option<Goal>, HandlerError> {
    let goal = load_or_migrate_goal(cwd, session)?;
    Ok(goal.filter(|goal| goal.completed_at.is_none()))
}

/// `loadOrMigrateRunningGoal`.
pub fn load_or_migrate_running_goal(
    cwd: &JsString,
    session: &ReadonlySessionManager,
) -> Result<Option<Goal>, HandlerError> {
    let goal = load_or_migrate_active_goal(cwd, session)?;
    Ok(goal.filter(|goal| goal.paused_at.is_none()))
}

fn migrate_legacy_goal(cwd: &JsString, session: &ReadonlySessionManager) -> Result<Option<Goal>, HandlerError> {
    for file in [goal_path(cwd, session)?, old_project_goal_path(cwd)] {
        let Some(goal) = load_goal_file(&file) else {
            continue;
        };
        save_goal_json(session, &goal)?;
        remove_file(&file)?;
        return Ok(Some(goal));
    }
    Ok(None)
}

/// `saveGoal`.
pub fn save_goal(session: &ReadonlySessionManager, goal: &Goal) -> Result<(), HandlerError> {
    save_goal_json(session, goal)
}

/// `markGoalComplete`.
pub fn mark_goal_complete(
    cwd: &JsString,
    session: &ReadonlySessionManager,
    reason: impl Into<JsString>,
) -> Result<Option<Goal>, HandlerError> {
    let Some(goal) = load_or_migrate_active_goal(cwd, session)? else {
        return Ok(None);
    };
    let completed_goal = Goal {
        completed_at: Some(crate::utils::iso_time::now_iso_string().into()),
        completion_reason: Some(reason.into()),
        review_evidence: None,
        ..goal
    };
    save_goal(session, &completed_goal)?;
    Ok(Some(completed_goal))
}

/// `pauseGoal`.
pub fn pause_goal(
    cwd: &JsString,
    session: &ReadonlySessionManager,
    reason: impl Into<JsString>,
) -> Result<Option<Goal>, HandlerError> {
    let Some(goal) = load_or_migrate_active_goal(cwd, session)? else {
        return Ok(None);
    };
    let paused_goal = Goal {
        paused_at: Some(
            goal.paused_at
                .clone()
                .unwrap_or_else(|| crate::utils::iso_time::now_iso_string().into()),
        ),
        pause_reason: Some(reason.into()),
        ..goal
    };
    save_goal(session, &paused_goal)?;
    Ok(Some(paused_goal))
}

/// `resumeGoal`.
pub fn resume_goal(cwd: &JsString, session: &ReadonlySessionManager) -> Result<Option<Goal>, HandlerError> {
    let Some(goal) = load_or_migrate_active_goal(cwd, session)? else {
        return Ok(None);
    };
    if goal.paused_at.is_none() {
        return Ok(None);
    }
    let resumed_goal = Goal {
        paused_at: None,
        pause_reason: None,
        ..goal
    };
    save_goal(session, &resumed_goal)?;
    Ok(Some(resumed_goal))
}

/// `clearGoal`.
pub fn clear_goal(cwd: &JsString, session: &ReadonlySessionManager) -> Result<bool, HandlerError> {
    let has_stored_goal = session
        .get_session_goal_json()
        .map_err(|error| HandlerError::new(error.message().clone()))?
        .is_some();
    let legacy_file = goal_path(cwd, session)?;
    let has_legacy_goal = file_exists(&legacy_file);
    session
        .clear_session_goal_json()
        .map_err(|error| HandlerError::new(error.message().clone()))?;
    if has_legacy_goal {
        remove_file(&legacy_file)?;
    }
    Ok(has_stored_goal || has_legacy_goal)
}

/// `loadPreviousGoalFile`.
pub fn load_previous_goal_file(cwd: &JsString, session_id: &JsString) -> Result<Option<Goal>, HandlerError> {
    Ok(load_goal_file(&goal_path_for_session_id(cwd, session_id)?))
}
