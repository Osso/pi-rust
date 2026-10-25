//! `cli/session-picker.ts` session loading/filtering/ordering.

use pi_coding_agent::cli::session_picker::SessionListProgress;
use pi_coding_agent::cli::session_picker::{SessionListFuture, SessionLoader, load_sessions};
use pi_coding_agent::core::session_manager::SessionInfo;
use pi_js_compat::js_json::JsString;

fn session(path: &str, name: Option<&str>, modified: i64, archived: bool) -> SessionInfo {
    SessionInfo {
        path: JsString::from(path.to_owned()),
        id: JsString::from(path.to_owned()),
        cwd: JsString::from("/work"),
        name: name.map(str::to_owned).map(JsString::from),
        is_archived: Some(archived),
        parent_session_path: None,
        created: modified as f64,
        modified: modified as f64,
        message_count: 0,
        first_message: JsString::default(),
    }
}

struct FixedLoader {
    sessions: Vec<SessionInfo>,
}

impl SessionLoader for FixedLoader {
    fn load<'a>(
        &'a self,
        on_progress: Option<SessionListProgress<'a>>,
    ) -> SessionListFuture<'a, Result<Vec<SessionInfo>, JsString>> {
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

fn paths(sessions: &[SessionInfo]) -> Vec<&JsString> {
    sessions.iter().map(|session| &session.path).collect()
}

#[tokio::test]
async fn hides_resident_sessions_from_current_and_archived_lists() {
    let mut supervisor = session("/s/canonical.jsonl", Some("Supervisor"), 90, false);
    supervisor.id = "supervisor".into();
    let mut architect = session("/s/archived-canonical.jsonl", Some("Architect"), 100, true);
    architect.id = "architect".into();
    let loader = loader(vec![
        supervisor,
        architect,
        session("/s/supervisor-sessions/renamed.jsonl", Some("Renamed"), 80, false),
        session("/s/architect-sessions/old.jsonl", Some("Old"), 70, true),
        session("/s/current.jsonl", Some("Current"), 10, false),
        session("/s/archive.jsonl", None, 20, true),
    ]);
    assert_eq!(
        paths(&load_sessions(&loader, None, false).await.unwrap()),
        ["/s/current.jsonl"]
    );
    assert_eq!(
        paths(&load_sessions(&loader, None, true).await.unwrap()),
        ["/s/archive.jsonl"]
    );
}

#[tokio::test]
async fn filters_by_archive_flag_and_forwards_progress() {
    let loader = loader(vec![
        session("/s/current.jsonl", None, 10, false),
        session("/s/archived.jsonl", None, 20, true),
    ]);
    let mut progress_calls = Vec::new();
    let mut on_progress = |current: usize, total: usize| progress_calls.push((current, total));

    let current = load_sessions(&loader, Some(&mut on_progress), false).await.unwrap();
    assert_eq!(paths(&current), vec!["/s/current.jsonl"]);
    assert_eq!(progress_calls, vec![(1, 2), (2, 2)]);

    let archived = load_sessions(&loader, None, true).await.unwrap();
    assert_eq!(paths(&archived), vec!["/s/archived.jsonl"]);
}

#[tokio::test]
async fn metadata_names_order_named_sessions_first_then_by_modification_time() {
    let loader = loader(vec![
        session("/s/newest.jsonl", Some("Newest named"), 30, false),
        session("/s/older.jsonl", Some("Older named"), 20, false),
        session("/s/unnamed.jsonl", None, 40, false),
        session("/s/archived.jsonl", Some("Archived"), 50, true),
    ]);
    let sessions = load_sessions(&loader, None, false).await.unwrap();
    assert_eq!(
        paths(&sessions),
        vec!["/s/newest.jsonl", "/s/older.jsonl", "/s/unnamed.jsonl"],
        "named sessions come first (modified order within each group)"
    );

    let archived = load_sessions(&loader, None, true).await.unwrap();
    assert_eq!(paths(&archived), vec!["/s/archived.jsonl"]);
}

#[tokio::test]
async fn blank_names_do_not_count_as_named() {
    let loader = loader(vec![
        session("/s/blank.jsonl", Some("   "), 40, false),
        session("/s/named.jsonl", Some("Named"), 10, false),
    ]);
    let sessions = load_sessions(&loader, None, false).await.unwrap();
    assert_eq!(paths(&sessions), vec!["/s/named.jsonl", "/s/blank.jsonl"]);
}

#[tokio::test]
async fn archived_sessions_are_ordered_only_by_modification_time() {
    let loader = loader(vec![
        session("/s/old-named.jsonl", Some("Named"), 10, true),
        session("/s/new-unnamed.jsonl", None, 99, true),
    ]);
    let sessions = load_sessions(&loader, None, true).await.unwrap();
    assert_eq!(paths(&sessions), vec!["/s/new-unnamed.jsonl", "/s/old-named.jsonl"]);
}

#[tokio::test]
async fn loader_errors_propagate() {
    struct FailingLoader;

    impl SessionLoader for FailingLoader {
        fn load<'a>(
            &'a self,
            _on_progress: Option<SessionListProgress<'a>>,
        ) -> SessionListFuture<'a, Result<Vec<SessionInfo>, JsString>> {
            Box::pin(async { Err(JsString::from("scan failed")) })
        }
    }

    let error = load_sessions(&FailingLoader, None, false)
        .await
        .expect_err("loader error propagates");
    assert_eq!(error, "scan failed");
}

#[tokio::test]
async fn unnamed_sessions_order_by_modification_time() {
    let loader = loader(vec![
        session("/s/b.jsonl", None, 10, false),
        session("/s/a.jsonl", None, 20, false),
    ]);
    let sessions = load_sessions(&loader, None, false).await.unwrap();
    assert_eq!(paths(&sessions), vec!["/s/a.jsonl", "/s/b.jsonl"]);
}
