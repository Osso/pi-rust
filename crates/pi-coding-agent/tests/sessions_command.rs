//! Ports `packages/coding-agent/test/sessions-command.test.ts`.

use std::sync::Mutex;

use pi_ai::request::BoxFuture;
use pi_coding_agent::cli::{SessionsCommandDependencies, handle_sessions_command};
use pi_coding_agent::core::session_tool_output::ToolResultSessionMigrationReport;

#[derive(Default)]
struct Harness {
    stdout: Mutex<Vec<String>>,
    stderr: Mutex<Vec<String>>,
    now_millis: i64,
    cutoff: Mutex<Option<String>>,
    archived: Vec<String>,
    truncate: Mutex<Option<ToolResultSessionMigrationReport>>,
    refreshed: Mutex<bool>,
}

impl SessionsCommandDependencies for Harness {
    fn stdout(&self, text: &str) {
        self.stdout.lock().unwrap().push(text.to_owned());
    }

    fn stderr(&self, text: &str) {
        self.stderr.lock().unwrap().push(text.to_owned());
    }

    fn now_millis(&self) -> i64 {
        self.now_millis
    }

    fn refresh_metadata<'a>(&'a self, _control_db_path: &'a str) -> BoxFuture<'a, Result<(), String>> {
        *self.refreshed.lock().unwrap() = true;
        Box::pin(async { Ok(()) })
    }

    fn archive_older_than(&self, _control_db_path: &str, cutoff_iso: &str) -> Result<Vec<String>, String> {
        *self.cutoff.lock().unwrap() = Some(cutoff_iso.to_owned());
        Ok(self.archived.clone())
    }

    fn agent_dir(&self) -> String {
        "/tmp/pi-agent".to_owned()
    }

    fn truncate_tool_output(&self, agent_dir: &str) -> ToolResultSessionMigrationReport {
        assert_eq!(agent_dir, "/tmp/pi-agent");
        self.truncate.lock().unwrap().take().unwrap_or_default()
    }
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn report(
    truncated_messages: usize,
    changed_files: usize,
    skipped_malformed: usize,
) -> ToolResultSessionMigrationReport {
    ToolResultSessionMigrationReport {
        scanned_files: 3,
        changed_files,
        truncated_messages,
        skipped_malformed_files: skipped_malformed,
        ..Default::default()
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn archives_sessions_using_a_five_day_default_cutoff() {
    runtime().block_on(async {
        let harness = Harness {
            now_millis: 1_783_641_600_000,
            archived: vec!["/tmp/old.jsonl".to_owned(), "/tmp/older.jsonl".to_owned()],
            ..Default::default()
        };
        let outcome = handle_sessions_command(&args(&["sessions", "archive"]), &harness)
            .await
            .unwrap();

        assert!(outcome.handled);
        assert_eq!(outcome.exit_code, None);
        assert_eq!(
            harness.cutoff.lock().unwrap().as_deref(),
            Some("2026-07-05T00:00:00.000Z")
        );
        assert_eq!(
            *harness.stdout.lock().unwrap(),
            vec!["Archived 2 sessions older than 5 days.\n"]
        );
        assert!(harness.stderr.lock().unwrap().is_empty());
        assert!(*harness.refreshed.lock().unwrap());
    });
}

#[test]
fn accepts_an_explicit_day_cutoff() {
    runtime().block_on(async {
        let harness = Harness {
            now_millis: 1_783_641_600_000,
            ..Default::default()
        };
        let outcome = handle_sessions_command(&args(&["sessions", "archive", "--older-than", "2"]), &harness)
            .await
            .unwrap();

        assert!(outcome.handled);
        assert_eq!(
            harness.cutoff.lock().unwrap().as_deref(),
            Some("2026-07-08T00:00:00.000Z")
        );
        assert_eq!(
            *harness.stdout.lock().unwrap(),
            vec!["Archived 0 sessions older than 2 days.\n"]
        );
    });
}

#[test]
fn rejects_invalid_archive_arguments() {
    runtime().block_on(async {
        for argv in [
            vec!["sessions", "archive", "--older-than", "0"],
            vec!["sessions", "archive", "--older-than", "-1"],
            vec!["sessions", "archive", "--older-than", "nope"],
            vec!["sessions", "archive", "--older-than"],
            vec!["sessions", "archive", "--older-than", "1", "extra"],
            vec!["sessions", "archive", "1"],
        ] {
            let harness = Harness {
                now_millis: 1_783_641_600_000,
                ..Default::default()
            };
            let outcome = handle_sessions_command(
                &argv.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>(),
                &harness,
            )
            .await
            .unwrap();
            assert!(outcome.handled, "argv: {argv:?}");
            assert_eq!(outcome.exit_code, Some(1), "argv: {argv:?}");
            assert!(harness.stdout.lock().unwrap().is_empty(), "argv: {argv:?}");
            assert_eq!(harness.stderr.lock().unwrap().len(), 1, "argv: {argv:?}");
        }
    });
}

#[test]
fn reports_tool_result_truncation_through_the_migration_command() {
    runtime().block_on(async {
        let harness = Harness::default();
        *harness.truncate.lock().unwrap() = Some(report(4, 2, 1));
        let outcome = handle_sessions_command(&args(&["sessions", "truncate-tool-output"]), &harness)
            .await
            .unwrap();

        assert!(outcome.handled);
        assert_eq!(outcome.exit_code, None);
        assert_eq!(
            *harness.stdout.lock().unwrap(),
            vec!["Truncated 4 tool results in 2 sessions. Skipped 1 file.\n"]
        );
        assert!(harness.stderr.lock().unwrap().is_empty());
    });
}

#[test]
fn reports_migration_errors_and_sets_a_failing_exit_code() {
    runtime().block_on(async {
        let harness = Harness::default();
        let mut migration = report(0, 0, 1);
        migration.errors = vec!["/tmp/broken.jsonl: malformed JSONL".to_owned()];
        *harness.truncate.lock().unwrap() = Some(migration);
        let outcome = handle_sessions_command(&args(&["sessions", "truncate-tool-output"]), &harness)
            .await
            .unwrap();

        assert!(outcome.handled);
        assert_eq!(outcome.exit_code, Some(1));
        assert_eq!(
            *harness.stdout.lock().unwrap(),
            vec!["Truncated 0 tool results in 0 sessions. Skipped 1 file.\n"]
        );
        assert_eq!(
            *harness.stderr.lock().unwrap(),
            vec!["Migration errors:\n/tmp/broken.jsonl: malformed JSONL\n"]
        );
    });
}

#[test]
fn prints_help_and_ignores_non_sessions_commands() {
    runtime().block_on(async {
        let harness = Harness::default();
        let outcome = handle_sessions_command(&args(&["sessions", "--help"]), &harness)
            .await
            .unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        assert!(harness.stdout.lock().unwrap()[0].starts_with("Usage:\n  pi sessions archive"));
        assert!(harness.stderr.lock().unwrap().is_empty());

        let harness = Harness::default();
        let outcome = handle_sessions_command(&args(&["sessions"]), &harness).await.unwrap();
        assert_eq!(outcome.exit_code, Some(1));
        assert!(harness.stdout.lock().unwrap().is_empty());
        assert_eq!(harness.stderr.lock().unwrap().len(), 1);

        let harness = Harness::default();
        let outcome = handle_sessions_command(&args(&["status"]), &harness).await.unwrap();
        assert!(!outcome.handled);
        assert_eq!(outcome.exit_code, None);
    });
}
