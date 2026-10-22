//! Port of `packages/coding-agent/src/core/session-cwd.ts`.

use crate::utils::paths::node_dirname;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::path::Path;

use crate::core::session_manager::{SessionManager, SessionManagerError, SetMetadataControlDbPathOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCwdIssue {
    pub session_file: Option<JsString>,
    pub session_cwd: JsString,
    pub fallback_cwd: JsString,
}

/// Session state needed to detect a missing stored cwd.
pub trait SessionCwdSource {
    fn get_cwd(&self) -> &JsString;
    fn get_session_file(&self) -> Option<&JsString>;
}

impl SessionCwdSource for SessionManager {
    fn get_cwd(&self) -> &JsString {
        SessionManager::get_cwd(self)
    }

    fn get_session_file(&self) -> Option<&JsString> {
        SessionManager::get_session_file(self)
    }
}

fn find_nearest_existing_cwd(cwd: &JsString) -> JsString {
    let mut candidate = cwd.clone();
    while !Path::new(&encode_node_utf8(&candidate)).exists() {
        let parent = node_dirname(&candidate);
        if parent == candidate {
            break;
        }
        candidate = parent;
    }
    candidate
}

pub fn get_missing_session_cwd_issue(
    session_manager: &dyn SessionCwdSource,
    fallback_cwd: &JsString,
) -> Option<SessionCwdIssue> {
    let session_file = session_manager.get_session_file()?;
    let session_cwd = session_manager.get_cwd();
    if session_cwd.is_empty() || Path::new(&encode_node_utf8(session_cwd)).exists() {
        return None;
    }
    Some(SessionCwdIssue {
        session_file: Some(session_file.to_owned()),
        session_cwd: session_cwd.to_owned(),
        fallback_cwd: find_nearest_existing_cwd(fallback_cwd),
    })
}

/// Reopen a session with an explicit cwd and persist the cwd override.
pub fn reopen_session_with_cwd(
    issue: &SessionCwdIssue,
    session_dir: Option<&JsString>,
    selected_cwd: &JsString,
    control_db_path: &JsString,
) -> Result<SessionManager, SessionManagerError> {
    let Some(session_file) = issue.session_file.as_ref() else {
        return Err(SessionManagerError::new(
            "Cannot reopen a session without a session file",
        ));
    };
    let mut session_manager = SessionManager::open(session_file, session_dir, Some(selected_cwd))?;
    session_manager.set_metadata_control_db_path(
        Some(control_db_path),
        SetMetadataControlDbPathOptions {
            cwd_override: Some(selected_cwd.to_owned()),
            ..Default::default()
        },
    )?;
    Ok(session_manager)
}

pub fn format_missing_session_cwd_error(issue: &SessionCwdIssue) -> JsString {
    let session_file = issue
        .session_file
        .as_ref()
        .map(|file| JsString::from("\nSession file: ") + file)
        .unwrap_or_default();
    JsString::from("Stored session working directory does not exist: ")
        + &issue.session_cwd
        + session_file
        + "\nCurrent working directory: "
        + &issue.fallback_cwd
}

pub fn format_missing_session_cwd_prompt(issue: &SessionCwdIssue) -> JsString {
    JsString::from("cwd from session file does not exist\n")
        + &issue.session_cwd
        + "\n\ncontinue in current cwd\n"
        + &issue.fallback_cwd
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingSessionCwdError {
    pub issue: SessionCwdIssue,
}

impl MissingSessionCwdError {
    pub const NAME: &'static str = "MissingSessionCwdError";

    pub fn message(&self) -> JsString {
        format_missing_session_cwd_error(&self.issue)
    }

    pub fn new(issue: SessionCwdIssue) -> Self {
        Self { issue }
    }
}

impl std::fmt::Display for MissingSessionCwdError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message()))
    }
}

impl std::error::Error for MissingSessionCwdError {}

pub fn assert_session_cwd_exists(
    session_manager: &dyn SessionCwdSource,
    fallback_cwd: &JsString,
) -> Result<(), MissingSessionCwdError> {
    match get_missing_session_cwd_issue(session_manager, fallback_cwd) {
        Some(issue) => Err(MissingSessionCwdError::new(issue)),
        None => Ok(()),
    }
}
