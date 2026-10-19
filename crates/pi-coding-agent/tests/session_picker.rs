//! `cli/session-picker.ts` session loading/filtering/naming/ordering.

use std::path::PathBuf;

use pi_coding_agent::cli::session_picker::SessionListProgress;
use pi_coding_agent::cli::session_picker::{SessionListFuture, SessionLoader, load_sessions_with_control_names};
use pi_coding_agent::core::session_control_db::set_named_session;
use pi_coding_agent::core::session_manager::SessionInfo;

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-session-picker-{name}-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn session(path: &str, name: Option<&str>, modified: i64, archived: bool) -> SessionInfo {
    SessionInfo {
        path: path.to_owned(),
        id: path.to_owned(),
        cwd: "/work".to_owned(),
        name: name.map(str::to_owned),
        is_archived: Some(archived),
        parent_session_path: None,
        created: modified,
        modified,
        message_count: 0,
        first_message: String::new(),
        all_messages_text: String::new(),
    }
}

struct FixedLoader {
    sessions: Vec<SessionInfo>,
}

impl SessionLoader for FixedLoader {
    fn load<'a>(
        &'a self,
        on_progress: Option<SessionListProgress<'a>>,
    ) -> SessionListFuture<'a, Result<Vec<SessionInfo>, String>> {
        Box::pin(async move {
            if let Some(progress) = on_progress {
                progress(1, 2);
                progress(2, 2);
            }
            Ok(self.sessions.clone())
        })
    }
}

fn loader(sessions: Vec<SessionInfo>) -> FixedLoader {
    FixedLoader { sessions }
}

#[tokio::test]
async fn filters_by_archive_flag_and_forwards_progress() {
    let loader = loader(vec![
        session("/s/current.jsonl", None, 10, false),
        session("/s/archived.jsonl", None, 20, true),
    ]);
    let mut progress_calls = Vec::new();
    let mut on_progress = |current: usize, total: usize| progress_calls.push((current, total));

    let current = load_sessions_with_control_names(&loader, None, Some(&mut on_progress), false)
        .await
        .unwrap();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].path, "/s/current.jsonl");
    assert_eq!(progress_calls, vec![(1, 2), (2, 2)]);

    let archived = load_sessions_with_control_names(&loader, None, None, true)
        .await
        .unwrap();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].path, "/s/archived.jsonl");
}

#[tokio::test]
async fn without_a_control_db_the_loader_order_is_kept() {
    let loader = loader(vec![
        session("/s/b.jsonl", None, 10, false),
        session("/s/a.jsonl", Some("kept"), 20, false),
    ]);
    let sessions = load_sessions_with_control_names(&loader, None, None, false)
        .await
        .unwrap();
    assert_eq!(
        sessions
            .iter()
            .map(|session| (session.path.as_str(), session.name.as_deref()))
            .collect::<Vec<_>>(),
        vec![("/s/b.jsonl", None), ("/s/a.jsonl", Some("kept"))]
    );
}

#[tokio::test]
async fn control_db_names_win_and_order_named_sessions_first() {
    let dir = temp_dir("names");
    let db = dir.join("control.sqlite").display().to_string();
    set_named_session(&db, "/s/newest.jsonl", "Newest named").unwrap();
    set_named_session(&db, "/s/older.jsonl", "Older named").unwrap();

    let loader = loader(vec![
        session("/s/newest.jsonl", None, 30, false),
        session("/s/older.jsonl", Some("file name"), 20, false),
        session("/s/unnamed.jsonl", None, 40, false),
        session("/s/archived.jsonl", None, 50, true),
    ]);
    let sessions = load_sessions_with_control_names(&loader, Some(&db), None, false)
        .await
        .unwrap();
    assert_eq!(
        sessions
            .iter()
            .map(|session| (session.path.as_str(), session.name.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            ("/s/newest.jsonl", Some("Newest named")),
            ("/s/older.jsonl", Some("Older named")),
            ("/s/unnamed.jsonl", None),
        ]
    );

    let archived = load_sessions_with_control_names(&loader, Some(&db), None, true)
        .await
        .unwrap();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].path, "/s/archived.jsonl");
}

#[tokio::test]
async fn archived_sessions_are_ordered_only_by_modification_time() {
    let dir = temp_dir("archived-order");
    let db = dir.join("control.sqlite").display().to_string();
    set_named_session(&db, "/s/old-named.jsonl", "Named").unwrap();

    let loader = loader(vec![
        session("/s/old-named.jsonl", None, 10, true),
        session("/s/new-unnamed.jsonl", None, 99, true),
    ]);
    let sessions = load_sessions_with_control_names(&loader, Some(&db), None, true)
        .await
        .unwrap();
    assert_eq!(
        sessions.iter().map(|session| session.path.as_str()).collect::<Vec<_>>(),
        vec!["/s/new-unnamed.jsonl", "/s/old-named.jsonl"]
    );
}

#[tokio::test]
async fn loader_errors_propagate() {
    struct FailingLoader;

    impl SessionLoader for FailingLoader {
        fn load<'a>(
            &'a self,
            _on_progress: Option<SessionListProgress<'a>>,
        ) -> SessionListFuture<'a, Result<Vec<SessionInfo>, String>> {
            Box::pin(async { Err("scan failed".to_owned()) })
        }
    }

    let error = load_sessions_with_control_names(&FailingLoader, None, None, false)
        .await
        .expect_err("loader error propagates");
    assert_eq!(error, "scan failed");
}

#[tokio::test]
async fn an_empty_control_db_still_orders_by_modification_time() {
    let dir = temp_dir("empty-db");
    let db = dir.join("control.sqlite").display().to_string();
    let loader = loader(vec![
        session("/s/b.jsonl", None, 10, false),
        session("/s/a.jsonl", Some("file name"), 20, false),
    ]);
    let sessions = load_sessions_with_control_names(&loader, Some(&db), None, false)
        .await
        .unwrap();
    assert_eq!(
        sessions
            .iter()
            .map(|session| (session.path.as_str(), session.name.as_deref()))
            .collect::<Vec<_>>(),
        vec![("/s/a.jsonl", Some("file name")), ("/s/b.jsonl", None)]
    );
}
