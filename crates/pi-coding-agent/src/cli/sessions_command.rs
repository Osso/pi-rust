//! `pi sessions` command (`cli/sessions-command.ts`).

use std::io::Write;

use pi_ai::request::BoxFuture;

use crate::config::{get_agent_dir, get_user_state_root};
use crate::core::session_control_db::{
    WritableSessionMetadata, archive_sessions_older_than, get_control_db_path, write_session_metadata,
};
use crate::core::session_manager::{SessionInfo, SessionManager};
use crate::core::session_tool_output::{ToolResultSessionMigrationReport, migrate_tool_result_session_files};
use crate::utils::iso_time::format_iso_millis;

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
    fn stdout(&self, text: &str) {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }

    fn stderr(&self, text: &str) {
        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(text.as_bytes());
        let _ = stderr.flush();
    }

    /// `new Date().getTime()`.
    fn now_millis(&self) -> i64 {
        now_millis()
    }

    /// Refresh control-database metadata from the session files.
    fn refresh_metadata<'a>(&'a self, control_db_path: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let sessions = SessionManager::list_all(None, None, None).map_err(|error| error.to_string())?;
            for session in sessions {
                write_session_metadata(control_db_path, &writable_metadata(&session))
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        })
    }

    fn archive_older_than(&self, control_db_path: &str, cutoff_iso: &str) -> Result<Vec<String>, String> {
        archive_sessions_older_than(control_db_path, cutoff_iso).map_err(|error| error.to_string())
    }

    fn agent_dir(&self) -> String {
        get_agent_dir().display().to_string()
    }

    fn truncate_tool_output(&self, agent_dir: &str) -> ToolResultSessionMigrationReport {
        migrate_tool_result_session_files(agent_dir, now_millis)
    }
}

fn writable_metadata(session: &SessionInfo) -> WritableSessionMetadata {
    WritableSessionMetadata {
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
        created_at: format_iso_millis(session.created),
        modified_at: format_iso_millis(session.modified),
        message_count: i64::try_from(session.message_count).unwrap_or(i64::MAX),
        first_message: session.first_message.clone(),
        all_messages_text: session.all_messages_text.clone(),
        index_message_text: None,
    }
}

fn default_control_db_path() -> String {
    get_control_db_path(&get_user_state_root())
}

/// `handleSessionsCommand`.
pub async fn handle_sessions_command(
    args: &[String],
    dependencies: &dyn SessionsCommandDependencies,
) -> Result<SessionsCommandOutcome, String> {
    if args.first().map(String::as_str) != Some("sessions") {
        return Ok(SessionsCommandOutcome {
            handled: false,
            exit_code: None,
        });
    }

    match args.get(1).map(String::as_str) {
        Some("truncate-tool-output") => Ok(handle_truncate(args, dependencies)),
        Some("archive") => handle_archive(args, dependencies).await,
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

fn handle_truncate(args: &[String], dependencies: &dyn SessionsCommandDependencies) -> SessionsCommandOutcome {
    if args.len() != 2 {
        print_sessions_help(dependencies, false);
        return SessionsCommandOutcome::failed();
    }

    let report = dependencies.truncate_tool_output(&dependencies.agent_dir());
    dependencies.stdout(&format_truncate_tool_output_report(&report));
    if report.errors.is_empty() {
        return SessionsCommandOutcome::handled();
    }

    dependencies.stderr(&format!("Migration errors:\n{}\n", report.errors.join("\n")));
    SessionsCommandOutcome::failed()
}

async fn handle_archive(
    args: &[String],
    dependencies: &dyn SessionsCommandDependencies,
) -> Result<SessionsCommandOutcome, String> {
    let Some(days) = parse_days(&args[2..]) else {
        print_sessions_help(dependencies, false);
        return Ok(SessionsCommandOutcome::failed());
    };

    let control_db_path = default_control_db_path();
    dependencies.refresh_metadata(&control_db_path).await?;
    let cutoff = dependencies.now_millis() as f64 - days * MILLISECONDS_PER_DAY;
    let archived = dependencies.archive_older_than(&control_db_path, &format_iso_millis(cutoff.trunc() as i64))?;
    let session_suffix = if archived.len() == 1 { "" } else { "s" };
    let day_suffix = if days == 1.0 { "" } else { "s" };
    dependencies.stdout(&format!(
        "Archived {} session{session_suffix} older than {} day{day_suffix}.\n",
        archived.len(),
        pi_js_compat::number::js_number_to_string(days),
    ));
    Ok(SessionsCommandOutcome::handled())
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

fn format_truncate_tool_output_report(report: &ToolResultSessionMigrationReport) -> String {
    let skipped = report.skipped_malformed_files + report.skipped_non_session_files + report.skipped_error_files;
    let skipped_text = if skipped > 0 {
        format!(" Skipped {skipped} file{}.", if skipped == 1 { "" } else { "s" })
    } else {
        String::new()
    };
    format!(
        "Truncated {} tool result{} in {} session{}.{skipped_text}\n",
        report.truncated_messages,
        if report.truncated_messages == 1 { "" } else { "s" },
        report.changed_files,
        if report.changed_files == 1 { "" } else { "s" },
    )
}

fn print_sessions_help(dependencies: &dyn SessionsCommandDependencies, to_stdout: bool) {
    let help = "Usage:\n  pi sessions archive [--older-than <days>]\n  pi sessions truncate-tool-output\n\nCommands:\n  archive                  Archive sessions older than the cutoff (default: 5 days).\n  truncate-tool-output    Rewrite oversized persisted tool results under the agent directory.\n                           Changed files receive a .tool-output-backup-* copy.\n";
    if to_stdout {
        dependencies.stdout(help);
    } else {
        dependencies.stderr(help);
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}
