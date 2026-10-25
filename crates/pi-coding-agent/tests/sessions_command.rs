//! Ports `packages/coding-agent/test/sessions-command.test.ts`.

use crate::shared_oracle as oracle;

use serde_json::json;
use std::sync::Mutex;

use pi_ai::request::BoxFuture;
use pi_coding_agent::cli::{SessionsCommandDependencies, handle_sessions_command};
use pi_coding_agent::core::session_control_db::SessionMetadata;
use pi_coding_agent::core::session_tool_output::ToolResultSessionMigrationReport;
use pi_js_compat::js_json::JsString;

#[derive(Default)]
struct Harness {
    stdout: Mutex<Vec<JsString>>,
    stderr: Mutex<Vec<JsString>>,
    now_millis: i64,
    cutoff: Mutex<Option<String>>,
    archived: Vec<JsString>,
    truncate: Mutex<Option<ToolResultSessionMigrationReport>>,
    refreshed: Mutex<bool>,
    sessions: Vec<SessionMetadata>,
    compressed: Mutex<Vec<JsString>>,
}

impl SessionsCommandDependencies for Harness {
    fn stdout(&self, text: &JsString) {
        self.stdout.lock().unwrap().push(text.clone());
    }

    fn stderr(&self, text: &JsString) {
        self.stderr.lock().unwrap().push(text.clone());
    }

    fn now_millis(&self) -> i64 {
        self.now_millis
    }

    fn refresh_metadata<'a>(
        &'a self,
        _control_db_path: &'a JsString,
    ) -> BoxFuture<'a, Result<(), pi_ai::request::CallbackError>> {
        *self.refreshed.lock().unwrap() = true;
        Box::pin(async { Ok(()) })
    }

    fn archive_older_than(&self, _control_db_path: &JsString, cutoff_iso: &str) -> Result<Vec<JsString>, JsString> {
        *self.cutoff.lock().unwrap() = Some(cutoff_iso.to_owned());
        Ok(self.archived.clone())
    }

    fn control_db_path(&self) -> Result<JsString, pi_ai::request::CallbackError> {
        Ok("/tmp/injected-control.sqlite".into())
    }

    fn list_archived_sessions(&self, _db: &JsString) -> Result<Vec<SessionMetadata>, pi_ai::request::CallbackError> {
        Ok(self.sessions.clone())
    }

    fn compress_archived_session(
        &self,
        _db: &JsString,
        path: &JsString,
    ) -> Result<JsString, pi_ai::request::CallbackError> {
        if path.ends_with("broken.jsonl") {
            return Err(JsString::from("compression failed").into());
        }
        let archived = path.clone() + ".zst";
        self.compressed.lock().unwrap().push(archived.clone());
        Ok(archived)
    }

    fn agent_dir(&self) -> Result<JsString, pi_ai::request::CallbackError> {
        Ok("/tmp/pi-agent".into())
    }

    fn truncate_tool_output(
        &self,
        agent_dir: &JsString,
    ) -> Result<ToolResultSessionMigrationReport, pi_ai::request::CallbackError> {
        assert_eq!(agent_dir.to_utf8().unwrap(), "/tmp/pi-agent");
        Ok(self.truncate.lock().unwrap().take().unwrap_or_default())
    }
}

fn lines(output: &Mutex<Vec<JsString>>) -> Vec<String> {
    output
        .lock()
        .unwrap()
        .iter()
        .map(|text| text.to_utf8().unwrap())
        .collect()
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
fn sessions_console_branches_match_frozen() {
    runtime().block_on(async {
        let mut source = oracle::TsOracle::spawn(
            "sessions-command-oracle.ts",
            "packages/coding-agent/src/cli/sessions-command.ts",
        );
        for argv in [
            vec!["other"],
            vec!["sessions"],
            vec!["sessions", "--help"],
            vec!["sessions", "-h"],
            vec!["sessions", "unknown"],
            vec!["sessions", "compress-archived"],
            vec!["sessions", "compress-archived", "--dry-run"],
            vec!["sessions", "compress-archived", "--invalid"],
            vec!["sessions", "archive"],
            vec!["sessions", "archive", "--older-than", "1.5"],
            vec!["sessions", "archive", "--older-than", "0"],
            vec!["sessions", "truncate-tool-output", "extra"],
        ] {
            let harness = Harness::default();
            let expected = source.request(&json!({"id": 1, "args": argv}));
            assert_eq!(expected["ok"], true, "{expected}");
            let actual = handle_sessions_command(&args(&argv), &harness).await.unwrap();
            assert_eq!(
                json!({
                    "handled": actual.handled, "exitCode": actual.exit_code,
                    "stdout": lines(&harness.stdout), "stderr": lines(&harness.stderr),
                }),
                expected["output"],
                "argv={argv:?}"
            );
        }
    });
}

fn archived_metadata(id: &str, path: &str) -> SessionMetadata {
    serde_json::from_value(json!({
        "id": id, "sessionPath": path, "cwd": "/work", "messageCount": 0, "firstMessage": "",
        "createdAt": "2020-01-01T00:00:00.000Z", "modifiedAt": "2020-01-01T00:00:00.000Z",
        "updatedAt": "2020-01-01T00:00:00.000Z",
    }))
    .unwrap()
}

#[test]
fn compression_reports_and_candidates_match_frozen() {
    runtime().block_on(async {
        let mut source = oracle::TsOracle::spawn(
            "sessions-command-oracle.ts",
            "packages/coding-agent/src/cli/sessions-command.ts",
        );
        for sessions in [
            vec![],
            vec![archived_metadata("one", "/sessions/one.jsonl")],
            vec![
                archived_metadata("good", "/sessions/good.jsonl"),
                archived_metadata("failed", "/sessions/broken.jsonl"),
                archived_metadata("packed", "/sessions/packed.jsonl.zst"),
                archived_metadata("supervisor", "/sessions/renamed.jsonl"),
                archived_metadata("architect", "/sessions/architect.jsonl"),
                archived_metadata("old-supervisor", "/sessions/supervisor-sessions/old.jsonl"),
                archived_metadata("old-architect", "C:\\sessions\\architect-sessions\\old.jsonl"),
                archived_metadata("lookalike", "/sessions/not-supervisor-sessions/valid.jsonl"),
            ],
        ] {
            for dry_run in [false, true] {
                let mut argv = vec!["sessions", "compress-archived"];
                if dry_run {
                    argv.push("--dry-run");
                }
                let harness = Harness {
                    sessions: sessions.clone(),
                    ..Default::default()
                };
                let expected = source.request(&json!({"id": 1, "args": argv, "sessions": sessions}));
                assert_eq!(expected["ok"], true, "{expected}");
                let actual = handle_sessions_command(&args(&argv), &harness).await.unwrap();
                assert_eq!(
                    json!({
                        "handled": actual.handled, "exitCode": actual.exit_code,
                        "stdout": lines(&harness.stdout), "stderr": lines(&harness.stderr),
                    }),
                    expected["output"],
                    "dry_run={dry_run}, sessions={sessions:?}"
                );
                if dry_run {
                    assert!(harness.compressed.lock().unwrap().is_empty());
                }
            }
        }
    });
}

#[test]
fn default_archive_compresses_the_file_and_relocates_metadata() {
    use pi_coding_agent::core::session_control_db::{
        WritableSessionMetadata, read_session_metadata, write_session_metadata,
    };
    use pi_coding_agent::core::session_transcript::read_session_transcript_bytes;
    struct Defaults;
    impl SessionsCommandDependencies for Defaults {}
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("old.jsonl");
    let db: JsString = directory.path().join("control.sqlite").to_str().unwrap().into();
    let bytes = b"{\"type\":\"session\",\"version\":3,\"id\":\"old\"}\n";
    std::fs::write(&source, bytes).unwrap();
    let metadata: WritableSessionMetadata = serde_json::from_value(json!({
        "sessionPath": source, "id": "old", "cwd": "/work",
        "createdAt": "2020-01-01T00:00:00.000Z", "modifiedAt": "2020-01-01T00:00:00.000Z",
        "messageCount": 0, "firstMessage": "",
    }))
    .unwrap();
    write_session_metadata(&db, &metadata).unwrap();
    let archived = Defaults.archive_older_than(&db, "2026-01-01T00:00:00.000Z").unwrap();
    let expected: JsString = format!("{}.zst", source.display()).into();
    assert_eq!(archived.as_slice(), std::slice::from_ref(&expected));
    assert!(!source.exists());
    assert_eq!(read_session_transcript_bytes(&expected).unwrap(), bytes);
    assert!(read_session_metadata(&db, metadata.session_path).unwrap().is_none());
    assert!(
        read_session_metadata(&db, expected)
            .unwrap()
            .unwrap()
            .archived_at
            .is_some()
    );
}

#[test]
fn archives_sessions_using_a_five_day_default_cutoff() {
    runtime().block_on(async {
        let harness = Harness {
            now_millis: 1_783_641_600_000,
            archived: vec!["/tmp/old.jsonl".into(), "/tmp/older.jsonl".into()],
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
        assert_eq!(lines(&harness.stdout), vec!["Archived 2 sessions older than 5 days.\n"]);
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
        assert_eq!(lines(&harness.stdout), vec!["Archived 0 sessions older than 2 days.\n"]);
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
            lines(&harness.stdout),
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
        migration.errors = vec!["/tmp/broken.jsonl: malformed JSONL".into()];
        *harness.truncate.lock().unwrap() = Some(migration);
        let outcome = handle_sessions_command(&args(&["sessions", "truncate-tool-output"]), &harness)
            .await
            .unwrap();

        assert!(outcome.handled);
        assert_eq!(outcome.exit_code, Some(1));
        assert_eq!(
            lines(&harness.stdout),
            vec!["Truncated 0 tool results in 0 sessions. Skipped 1 file.\n"]
        );
        assert_eq!(
            lines(&harness.stderr),
            vec!["Migration errors:\n/tmp/broken.jsonl: malformed JSONL\n"]
        );
    });
}

#[test]
fn migration_errors_reach_stderr_with_raw_utf16_units() {
    runtime().block_on(async {
        let harness = Harness::default();
        let mut migration = report(0, 0, 1);
        let path = JsString::from_utf16(vec![0x2f, 0xd800, 0x2e, 0x6a]);
        migration.errors = vec![path.clone() + ": malformed JSONL"];
        *harness.truncate.lock().unwrap() = Some(migration);
        handle_sessions_command(&args(&["sessions", "truncate-tool-output"]), &harness)
            .await
            .unwrap();

        let expected = JsString::from("Migration errors:\n") + &path + ": malformed JSONL\n";
        assert_eq!(harness.stderr.lock().unwrap().as_slice(), [expected]);
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
        assert!(lines(&harness.stdout)[0].starts_with("Usage:\n  pi sessions archive"));
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
