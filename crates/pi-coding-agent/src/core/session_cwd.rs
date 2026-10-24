//! Port of `packages/coding-agent/src/core/session-cwd.ts`.

use crate::utils::paths::node_dirname;
use pi_ai::request::CallbackError;
use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;
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

pub(crate) fn stat_cwd_directory(path: &JsString) -> Result<bool, CallbackError> {
    if let Some(error) = pi_js_compat::node_fs::node_path_argument_error(path) {
        return Err(CallbackError::from_error(
            error.message.clone(),
            NativeNodeError::from(error),
        ));
    }
    match std::fs::metadata(encode_node_utf8(path)) {
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(error) => {
            let error = pi_js_compat::node_fs::node_fs_error_record(&error, "stat", path);
            if matches!(error.code.as_str(), "ENOENT" | "ENOTDIR") {
                return Ok(false);
            }
            Err(CallbackError::from_error(
                error.message.clone(),
                NativeNodeError::from(error),
            ))
        }
    }
}

pub fn find_nearest_existing_cwd(cwd: &JsString) -> Result<JsString, CallbackError> {
    let mut candidate = cwd.clone();
    loop {
        if stat_cwd_directory(&candidate)? {
            return Ok(candidate);
        }
        let parent = node_dirname(&candidate);
        if parent == candidate {
            return Err((JsString::from("No existing directory found for cwd: ") + cwd).into());
        }
        candidate = parent;
    }
}

pub fn get_missing_session_cwd_issue(
    session_manager: &dyn SessionCwdSource,
    fallback_cwd: &JsString,
) -> Result<Option<SessionCwdIssue>, CallbackError> {
    let Some(session_file) = session_manager.get_session_file().filter(|file| !file.is_empty()) else {
        return Ok(None);
    };
    let session_cwd = session_manager.get_cwd();
    if session_cwd.is_empty() || Path::new(&encode_node_utf8(session_cwd)).exists() {
        return Ok(None);
    }
    Ok(Some(SessionCwdIssue {
        session_file: Some(session_file.to_owned()),
        session_cwd: session_cwd.to_owned(),
        fallback_cwd: find_nearest_existing_cwd(fallback_cwd)?,
    }))
}

/// Reopen a session with an explicit cwd and persist the cwd override.
pub fn reopen_session_with_cwd(
    issue: &SessionCwdIssue,
    session_dir: Option<&JsString>,
    selected_cwd: &JsString,
    control_db_path: &JsString,
) -> Result<SessionManager, SessionManagerError> {
    let Some(session_file) = issue.session_file.as_ref().filter(|file| !file.is_empty()) else {
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
        .filter(|file| !file.is_empty())
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

impl From<MissingSessionCwdError> for pi_ai::request::CallbackError {
    fn from(error: MissingSessionCwdError) -> Self {
        let mut issue = JsObject::new();
        if let Some(file) = &error.issue.session_file {
            issue.insert("sessionFile".into(), JsValue::String(file.clone()));
        }
        issue.insert("sessionCwd".into(), JsValue::String(error.issue.session_cwd.clone()));
        issue.insert("fallbackCwd".into(), JsValue::String(error.issue.fallback_cwd.clone()));
        let native = NativeNodeError::new(MissingSessionCwdError::NAME, error.message())
            .with_property("issue", JsValue::Object(issue));
        Self::from_rejection(native.into()).with_original(std::sync::Arc::new(error))
    }
}

pub fn assert_session_cwd_exists(
    session_manager: &dyn SessionCwdSource,
    fallback_cwd: &JsString,
) -> Result<(), CallbackError> {
    match get_missing_session_cwd_issue(session_manager, fallback_cwd)? {
        Some(issue) => Err(MissingSessionCwdError::new(issue).into()),
        None => Ok(()),
    }
}
