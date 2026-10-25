//! `pi sessions` command (`cli/sessions-command.ts`).

use pi_ai::request::CallbackError;
use std::io::Write;

use pi_ai::request::BoxFuture;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

use crate::config::{get_agent_dir, get_user_state_root};
use crate::core::session_archive_storage::archive_persisted_session;
use crate::core::session_control_db::{
    SessionMetadata, WritableSessionMetadata, archive_sessions_older_than, get_control_db_path,
    list_archived_session_metadata, write_session_metadata,
};
use crate::core::session_manager::{SessionInfo, SessionManager};
use crate::core::session_tool_output::{ToolResultSessionMigrationReport, migrate_tool_result_session_files};
use crate::utils::iso_time::format_iso_millis;

#[cfg(test)]
#[path = "sessions_command_date_tests.rs"]
mod date_tests;

const DEFAULT_ARCHIVE_DAYS: f64 = 5.0;
const MILLISECONDS_PER_DAY: f64 = 86_400_000.0;

/// Result of handling one CLI invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionsCommandOutcome {
    pub handled: bool,
    pub exit_code: Option<i32>,
}

impl SessionsCommandOutcome {
    const fn handled() -> Self {
        Self {
            handled: true,
            exit_code: None,
        }
    }

    const fn failed() -> Self {
        Self {
            handled: true,
            exit_code: Some(1),
        }
    }
}

/// Injectable effects used by [`handle_sessions_command`].
pub trait SessionsCommandDependencies {
    fn stdout(&self, text: &JsString) {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(encode_node_utf8(text).as_bytes());
        let _ = stdout.flush();
    }

    fn stderr(&self, text: &JsString) {
        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(encode_node_utf8(text).as_bytes());
        let _ = stderr.flush();
    }

    /// `new Date().getTime()`.
    fn now_millis(&self) -> i64 {
        now_millis()
    }

    /// Refresh control-database metadata from the session files.
    fn refresh_metadata<'a>(&'a self, control_db_path: &'a JsString) -> BoxFuture<'a, Result<(), CallbackError>> {
        Box::pin(async move {
            let sessions = SessionManager::list_all(None, None, None).map_err(CallbackError::from)?;
            for session in sessions {
                let metadata = writable_metadata(&session).map_err(|cause| JsString::from(cause.to_string()))?;
                write_session_metadata(control_db_path, &metadata).map_err(CallbackError::from)?;
            }
            Ok(())
        })
    }

    fn archive_older_than(&self, control_db_path: &JsString, cutoff_iso: &str) -> Result<Vec<JsString>, JsString> {
        archive_sessions_older_than(control_db_path, cutoff_iso)
            .map_err(|error| error.message().clone())?
            .iter()
            .map(|path| archive_persisted_session(control_db_path, path).map_err(|error| error.message().clone()))
            .collect()
    }

    fn control_db_path(&self) -> Result<JsString, CallbackError> {
        default_control_db_path()
    }

    fn list_archived_sessions(&self, control_db_path: &JsString) -> Result<Vec<SessionMetadata>, CallbackError> {
        list_archived_session_metadata(control_db_path).map_err(CallbackError::from)
    }

    fn compress_archived_session(
        &self,
        control_db_path: &JsString,
        path: &JsString,
    ) -> Result<JsString, CallbackError> {
        archive_persisted_session(control_db_path, path).map_err(CallbackError::from)
    }

    fn agent_dir(&self) -> Result<JsString, CallbackError> {
        Ok(get_agent_dir()?)
    }

    fn truncate_tool_output(&self, agent_dir: &JsString) -> Result<ToolResultSessionMigrationReport, CallbackError> {
        migrate_tool_result_session_files(agent_dir, || Ok(now_millis() as f64))
    }
}

fn writable_metadata(session: &SessionInfo) -> Result<WritableSessionMetadata, pi_js_compat::date::InvalidTimeValue> {
    Ok(WritableSessionMetadata {
        session_path: session.path.clone(),
        id: session.id.clone(),
        cwd: session.cwd.clone(),
        name: session.name.clone(),
        parent_session_path: session.parent_session_path.clone(),
        archived_at: None,
        is_archived: session.is_archived,
        is_subagent: None,
        subagent_name: None,
        model_provider: None,
        model_id: None,
        thinking_level: None,
        created_at: pi_js_compat::date::to_iso_string_value(session.created)?.into(),
        modified_at: pi_js_compat::date::to_iso_string_value(session.modified)?.into(),
        message_count: i64::try_from(session.message_count).unwrap_or(i64::MAX),
        first_message: session.first_message.clone(),
    })
}

fn default_control_db_path() -> Result<JsString, CallbackError> {
    Ok(get_control_db_path(&get_user_state_root()?).into())
}

/// `handleSessionsCommand`.
pub async fn handle_sessions_command(
    args: &[String],
    dependencies: &dyn SessionsCommandDependencies,
) -> Result<SessionsCommandOutcome, CallbackError> {
    if args.first().map(String::as_str) != Some("sessions") {
        return Ok(SessionsCommandOutcome {
            handled: false,
            exit_code: None,
        });
    }

    match args.get(1).map(String::as_str) {
        Some("truncate-tool-output") => handle_truncate(args, dependencies),
        Some("archive") => handle_archive(args, dependencies).await,
        Some("compress-archived") => handle_compress(&args[2..], dependencies),
        _ => {
            let help = matches!(args.get(1).map(String::as_str), Some("--help" | "-h"));
            print_sessions_help(dependencies, help);
            Ok(SessionsCommandOutcome {
                handled: true,
                exit_code: Some(if help { 0 } else { 1 }),
            })
        }
    }
}

fn handle_truncate(
    args: &[String],
    dependencies: &dyn SessionsCommandDependencies,
) -> Result<SessionsCommandOutcome, CallbackError> {
    if args.len() != 2 {
        print_sessions_help(dependencies, false);
        return Ok(SessionsCommandOutcome::failed());
    }

    let report = dependencies.truncate_tool_output(&dependencies.agent_dir()?)?;
    dependencies.stdout(&format_truncate_tool_output_report(&report));
    if report.errors.is_empty() {
        return Ok(SessionsCommandOutcome::handled());
    }

    dependencies.stderr(&js_format!(
        "Migration errors:\n{}\n",
        JsString::join(report.errors.iter(), "\n")
    ));
    Ok(SessionsCommandOutcome::failed())
}

async fn handle_archive(
    args: &[String],
    dependencies: &dyn SessionsCommandDependencies,
) -> Result<SessionsCommandOutcome, CallbackError> {
    let Some(days) = parse_days(&args[2..]) else {
        print_sessions_help(dependencies, false);
        return Ok(SessionsCommandOutcome::failed());
    };

    let control_db_path = dependencies.control_db_path()?;
    dependencies.refresh_metadata(&control_db_path).await?;
    let cutoff = dependencies.now_millis() as f64 - days * MILLISECONDS_PER_DAY;
    let archived = dependencies.archive_older_than(&control_db_path, &format_iso_millis(cutoff.trunc() as i64))?;
    let session_suffix = if archived.len() == 1 { "" } else { "s" };
    let day_suffix = if days == 1.0 { "" } else { "s" };
    dependencies.stdout(&js_format!(
        "Archived {} session{} older than {} day{}.\n",
        archived.len(),
        session_suffix,
        pi_js_compat::number::js_number_to_string(days),
        day_suffix
    ));
    Ok(SessionsCommandOutcome::handled())
}

fn handle_compress(
    args: &[String],
    dependencies: &dyn SessionsCommandDependencies,
) -> Result<SessionsCommandOutcome, CallbackError> {
    let dry_run = match args {
        [] => false,
        [flag] if flag == "--dry-run" => true,
        _ => {
            print_sessions_help(dependencies, false);
            return Ok(SessionsCommandOutcome::failed());
        }
    };
    let db = dependencies.control_db_path()?;
    let sessions = dependencies.list_archived_sessions(&db)?;
    let (candidates, skipped): (Vec<_>, Vec<_>) = sessions.iter().partition(|session| should_compress(session));
    let (migrated, failures) = compress_candidates(&db, &candidates, dry_run, dependencies);
    print_compression_report(dry_run, &migrated, &skipped, &failures, dependencies);
    Ok(if failures.is_empty() {
        SessionsCommandOutcome::handled()
    } else {
        SessionsCommandOutcome::failed()
    })
}

fn should_compress(session: &SessionMetadata) -> bool {
    if !session.session_path.ends_with(".jsonl") || session.id == "supervisor" || session.id == "architect" {
        return false;
    }
    let has_resident_directory = session.session_path.split('/').any(|part| {
        part.split('\\')
            .any(|segment| segment == "supervisor-sessions" || segment == "architect-sessions")
    });
    !has_resident_directory
}

fn compress_candidates(
    db: &JsString,
    candidates: &[&SessionMetadata],
    dry_run: bool,
    dependencies: &dyn SessionsCommandDependencies,
) -> (Vec<JsString>, Vec<JsString>) {
    if dry_run {
        return (
            candidates.iter().map(|session| session.session_path.clone()).collect(),
            Vec::new(),
        );
    }
    let mut migrated = Vec::new();
    let mut failures = Vec::new();
    for session in candidates {
        match dependencies.compress_archived_session(db, &session.session_path) {
            Ok(path) => migrated.push(path),
            Err(error) => failures.push(js_format!("{}: {}", session.session_path, error.message())),
        }
    }
    (migrated, failures)
}

fn print_compression_report(
    dry_run: bool,
    migrated: &[JsString],
    skipped: &[&SessionMetadata],
    failures: &[JsString],
    dependencies: &dyn SessionsCommandDependencies,
) {
    let action = if dry_run { "Would migrate" } else { "Migrated" };
    dependencies.stdout(&js_format!(
        "{} {} archived session{}. Skipped {}. Failed {}.\n",
        action,
        migrated.len(),
        plural_suffix(migrated.len()),
        skipped.len(),
        failures.len()
    ));
    if !migrated.is_empty() {
        dependencies.stdout(&js_format!("{}:\n{}\n", action, JsString::join(migrated.iter(), "\n")));
    }
    if !skipped.is_empty() {
        let paths = JsString::join(skipped.iter().map(|session| &session.session_path), "\n");
        dependencies.stdout(&js_format!("Skipped:\n{}\n", paths));
    }
    if !failures.is_empty() {
        let failures = JsString::join(failures.iter(), "\n");
        dependencies.stderr(&js_format!("Archived-session migration failures:\n{}\n", failures));
        dependencies.stdout(&js_format!("Failed:\n{}\n", failures));
    }
}

fn parse_days(args: &[String]) -> Option<f64> {
    if args.is_empty() {
        return Some(DEFAULT_ARCHIVE_DAYS);
    }
    if args.len() != 2 || args[0] != "--older-than" {
        return None;
    }
    let days = pi_js_compat::number::parse_number(&args[1]);
    (days.is_finite() && days > 0.0).then_some(days)
}

fn plural_suffix(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn format_truncate_tool_output_report(report: &ToolResultSessionMigrationReport) -> JsString {
    let skipped = report.skipped_malformed_files + report.skipped_non_session_files + report.skipped_error_files;
    let skipped_text = if skipped > 0 {
        js_format!(" Skipped {} file{}.", skipped, plural_suffix(skipped))
    } else {
        JsString::default()
    };
    js_format!(
        "Truncated {} tool result{} in {} session{}.{}\n",
        report.truncated_messages,
        plural_suffix(report.truncated_messages),
        report.changed_files,
        plural_suffix(report.changed_files),
        skipped_text
    )
}

fn print_sessions_help(dependencies: &dyn SessionsCommandDependencies, to_stdout: bool) {
    let help = "Usage:\n  pi sessions archive [--older-than <days>]\n  pi sessions compress-archived [--dry-run]\n  pi sessions truncate-tool-output\n\nCommands:\n  archive                  Archive sessions older than the cutoff (default: 5 days).\n  compress-archived       Compress existing archived .jsonl sessions as .jsonl.zst.\n  truncate-tool-output    Rewrite oversized persisted tool results under the agent directory.\n                           Changed files receive a .tool-output-backup-* copy.\n";
    let help = JsString::from(help);
    if to_stdout {
        dependencies.stdout(&help);
    } else {
        dependencies.stderr(&help);
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}
