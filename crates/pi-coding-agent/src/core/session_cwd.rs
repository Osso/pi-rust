//! Port of `packages/coding-agent/src/core/session-cwd.ts`.

use std::path::{Path, PathBuf};

use crate::core::session_manager::{SessionManager, SessionManagerError, SetMetadataControlDbPathOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCwdIssue {
    pub session_file: Option<String>,
    pub session_cwd: String,
    pub fallback_cwd: String,
}

/// Session state needed to detect a missing stored cwd.
pub trait SessionCwdSource {
    fn get_cwd(&self) -> &str;
    fn get_session_file(&self) -> Option<&str>;
}

impl SessionCwdSource for SessionManager {
    fn get_cwd(&self) -> &str {
        SessionManager::get_cwd(self)
    }

    fn get_session_file(&self) -> Option<&str> {
        SessionManager::get_session_file(self)
    }
}

fn find_nearest_existing_cwd(cwd: &str) -> String {
    let mut candidate = PathBuf::from(cwd);
    while !candidate.exists() {
        let Some(parent) = candidate.parent() else {
            return candidate.display().to_string();
        };
        if parent == candidate {
            return candidate.display().to_string();
        }
        candidate = parent.to_path_buf();
    }
    candidate.display().to_string()
}

pub fn get_missing_session_cwd_issue(
    session_manager: &dyn SessionCwdSource,
    fallback_cwd: &str,
) -> Option<SessionCwdIssue> {
    let session_file = session_manager.get_session_file()?;
    let session_cwd = session_manager.get_cwd();
    if session_cwd.is_empty() || Path::new(session_cwd).exists() {
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
    session_dir: Option<&str>,
    selected_cwd: &str,
    control_db_path: &str,
) -> Result<SessionManager, SessionManagerError> {
    let Some(session_file) = issue.session_file.as_deref() else {
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

pub fn format_missing_session_cwd_error(issue: &SessionCwdIssue) -> String {
    let session_file = issue
        .session_file
        .as_deref()
        .map(|file| format!("\nSession file: {file}"))
        .unwrap_or_default();
    format!(
        "Stored session working directory does not exist: {}{session_file}\nCurrent working directory: {}",
        issue.session_cwd, issue.fallback_cwd
    )
}

pub fn format_missing_session_cwd_prompt(issue: &SessionCwdIssue) -> String {
    format!(
        "cwd from session file does not exist\n{}\n\ncontinue in current cwd\n{}",
        issue.session_cwd, issue.fallback_cwd
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingSessionCwdError {
    pub issue: SessionCwdIssue,
}

impl MissingSessionCwdError {
    pub const NAME: &'static str = "MissingSessionCwdError";

    pub fn new(issue: SessionCwdIssue) -> Self {
        Self { issue }
    }
}

impl std::fmt::Display for MissingSessionCwdError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&format_missing_session_cwd_error(&self.issue))
    }
}

impl std::error::Error for MissingSessionCwdError {}

pub fn assert_session_cwd_exists(
    session_manager: &dyn SessionCwdSource,
    fallback_cwd: &str,
) -> Result<(), MissingSessionCwdError> {
    match get_missing_session_cwd_issue(session_manager, fallback_cwd) {
        Some(issue) => Err(MissingSessionCwdError::new(issue)),
        None => Ok(()),
    }
}
