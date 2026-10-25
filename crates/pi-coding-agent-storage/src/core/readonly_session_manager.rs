//! Extension-facing live session access with owned read snapshots.

use super::session_manager::{SessionEntry, SessionHeader, SessionManager, SessionManagerError, SessionTreeNode};
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex, MutexGuard};

/// Frozen readonly view: isolated reads plus the two explicitly allowed goal mutators.
#[derive(Clone)]
pub struct ReadonlySessionManager {
    session: Arc<Mutex<SessionManager>>,
}

pub fn create_readonly_session_manager(session: Arc<Mutex<SessionManager>>) -> ReadonlySessionManager {
    ReadonlySessionManager { session }
}

impl ReadonlySessionManager {
    fn lock_session(&self) -> MutexGuard<'_, SessionManager> {
        self.session.lock().expect("session manager poisoned")
    }

    pub fn get_cwd(&self) -> JsString {
        self.lock_session().get_cwd().clone()
    }
    pub fn get_session_dir(&self) -> JsString {
        self.lock_session().get_session_dir().clone()
    }
    pub fn get_session_id(&self) -> JsString {
        self.lock_session().get_session_id().clone()
    }
    pub fn get_session_file(&self) -> Option<JsString> {
        self.lock_session().get_session_file().cloned()
    }
    pub fn get_leaf_id(&self) -> Option<JsString> {
        self.lock_session().get_leaf_id().cloned()
    }
    pub fn get_leaf_entry(&self) -> Option<SessionEntry> {
        self.lock_session().get_leaf_entry().cloned()
    }
    pub fn get_entry(&self, id: &JsString) -> Option<SessionEntry> {
        self.lock_session().get_entry(id).cloned()
    }
    pub fn get_label(&self, id: &JsString) -> Option<JsString> {
        self.lock_session().get_label(id).cloned()
    }
    pub fn get_branch(&self, from_id: Option<&JsString>) -> Vec<SessionEntry> {
        self.lock_session().get_branch(from_id)
    }
    pub fn build_context_entries(&self) -> Vec<SessionEntry> {
        self.lock_session().build_context_entries()
    }
    pub fn get_header(&self) -> Option<SessionHeader> {
        self.lock_session().get_header().cloned()
    }
    pub fn get_entries(&self) -> Vec<SessionEntry> {
        self.lock_session().get_entries()
    }
    pub fn get_tree(&self) -> Vec<SessionTreeNode> {
        self.lock_session().get_tree()
    }
    pub fn get_session_name(&self) -> Option<JsString> {
        self.lock_session().get_session_name()
    }
    pub fn has_session_name_state(&self) -> bool {
        self.lock_session().has_session_name_state()
    }
    pub fn get_session_goal_json(&self) -> Result<Option<JsString>, SessionManagerError> {
        self.lock_session().get_session_goal_json()
    }
    pub fn get_session_goal_json_for_session(&self, file: &JsString) -> Result<Option<JsString>, SessionManagerError> {
        self.lock_session().get_session_goal_json_for_session(file)
    }
    pub fn set_session_goal_json(&self, goal: &JsString) -> Result<(), SessionManagerError> {
        self.lock_session().set_session_goal_json(goal)
    }
    pub fn clear_session_goal_json(&self) -> Result<(), SessionManagerError> {
        self.lock_session().clear_session_goal_json()
    }
    pub fn is_subagent_session(&self) -> bool {
        self.lock_session().is_subagent_session()
    }
    pub fn get_subagent_name(&self) -> Option<JsString> {
        self.lock_session().get_subagent_name().cloned()
    }
}
