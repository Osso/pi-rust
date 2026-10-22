//! Port of `packages/coding-agent/extensions/goal/src/goal-state.ts`.

use std::path::{Path, PathBuf};

use pi_js_compat::js_json::{self, JsString};
use pi_js_compat::string::encode_uri_component;

use crate::core::extensions::types::HandlerError;
use crate::core::session_manager::SessionManager;

use super::goal_parsing::{parse_goal, parse_goal_json};
use super::goal_types::Goal;

fn goal_path_for_session_id(cwd: &str, session_id: &str) -> PathBuf {
    Path::new(cwd)
        .join(".pi")
        .join("goals")
        .join(format!("{}.json", encode_uri_component(session_id)))
}

fn goal_path(cwd: &str, session: &SessionManager) -> PathBuf {
    goal_path_for_session_id(cwd, session.get_session_id())
}

fn old_project_goal_path(cwd: &str) -> PathBuf {
    Path::new(cwd).join(".pi").join("goal.json")
}

fn save_goal_json(session: &mut SessionManager, goal: &Goal) -> Result<(), HandlerError> {
    let serialized = serde_json::to_string(goal).map_err(|error| HandlerError::new(error.to_string()))?;
    session
        .set_session_goal_json(&format!("{serialized}\n"))
        .map_err(|error| HandlerError::new(error.to_string()))
}

fn load_goal_file(file: &Path) -> Option<Goal> {
    if !file.exists() {
        return None;
    }
    let content = std::fs::read_to_string(file).ok()?;
    let value = js_json::parse_lossless(&content).ok()?;
    parse_goal(&value)
}

/// `loadOrMigrateGoal`.
pub fn load_or_migrate_goal(cwd: &str, session: &mut SessionManager) -> Result<Option<Goal>, HandlerError> {
    if let Some(stored_goal) = session
        .get_session_goal_json()
        .map_err(|error| HandlerError::new(error.to_string()))?
    {
        return Ok(parse_goal_json(&stored_goal));
    }
    migrate_legacy_goal(cwd, session)
}

/// `loadOrMigrateActiveGoal`.
pub fn load_or_migrate_active_goal(cwd: &str, session: &mut SessionManager) -> Result<Option<Goal>, HandlerError> {
    let goal = load_or_migrate_goal(cwd, session)?;
    Ok(goal.filter(|goal| goal.completed_at.is_none()))
}

/// `loadOrMigrateRunningGoal`.
pub fn load_or_migrate_running_goal(cwd: &str, session: &mut SessionManager) -> Result<Option<Goal>, HandlerError> {
    let goal = load_or_migrate_active_goal(cwd, session)?;
    Ok(goal.filter(|goal| goal.paused_at.is_none()))
}

fn migrate_legacy_goal(cwd: &str, session: &mut SessionManager) -> Result<Option<Goal>, HandlerError> {
    for file in [goal_path(cwd, session), old_project_goal_path(cwd)] {
        let Some(goal) = load_goal_file(&file) else {
            continue;
        };
        save_goal_json(session, &goal)?;
        std::fs::remove_file(&file).map_err(|error| HandlerError::new(error.to_string()))?;
        return Ok(Some(goal));
    }
    Ok(None)
}

/// `saveGoal`.
pub fn save_goal(session: &mut SessionManager, goal: &Goal) -> Result<(), HandlerError> {
    save_goal_json(session, goal)
}

/// `markGoalComplete`.
pub fn mark_goal_complete(
    cwd: &str,
    session: &mut SessionManager,
    reason: impl Into<JsString>,
) -> Result<Option<Goal>, HandlerError> {
    let Some(goal) = load_or_migrate_active_goal(cwd, session)? else {
        return Ok(None);
    };
    let completed_goal = Goal {
        completed_at: Some(crate::utils::iso_time::now_iso_string()),
        completion_reason: Some(reason.into()),
        review_evidence: None,
        ..goal
    };
    save_goal(session, &completed_goal)?;
    Ok(Some(completed_goal))
}

/// `pauseGoal`.
pub fn pause_goal(
    cwd: &str,
    session: &mut SessionManager,
    reason: impl Into<JsString>,
) -> Result<Option<Goal>, HandlerError> {
    let Some(goal) = load_or_migrate_active_goal(cwd, session)? else {
        return Ok(None);
    };
    let paused_goal = Goal {
        paused_at: Some(
            goal.paused_at
                .clone()
                .unwrap_or_else(crate::utils::iso_time::now_iso_string),
        ),
        pause_reason: Some(reason.into()),
        ..goal
    };
    save_goal(session, &paused_goal)?;
    Ok(Some(paused_goal))
}

/// `resumeGoal`.
pub fn resume_goal(cwd: &str, session: &mut SessionManager) -> Result<Option<Goal>, HandlerError> {
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
pub fn clear_goal(cwd: &str, session: &mut SessionManager) -> Result<bool, HandlerError> {
    let has_stored_goal = session
        .get_session_goal_json()
        .map_err(|error| HandlerError::new(error.to_string()))?
        .is_some();
    let legacy_file = goal_path(cwd, session);
    let has_legacy_goal = legacy_file.exists();
    session
        .clear_session_goal_json()
        .map_err(|error| HandlerError::new(error.to_string()))?;
    if has_legacy_goal {
        std::fs::remove_file(&legacy_file).map_err(|error| HandlerError::new(error.to_string()))?;
    }
    Ok(has_stored_goal || has_legacy_goal)
}

/// `loadPreviousGoalFile`.
pub fn load_previous_goal_file(cwd: &str, session_id: &str) -> Option<Goal> {
    load_goal_file(&goal_path_for_session_id(cwd, session_id))
}
