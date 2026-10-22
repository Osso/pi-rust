use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::Arc;

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::session::jsonl::types::NativeFileSystem;
use pi_agent_core::harness::session::{
    InMemorySessionStorage, JsonlSessionCreateOptions, JsonlSessionListOptions, JsonlSessionMetadata, JsonlSessionRepo,
    JsonlSessionRepoOptions, Session, SessionMetadata, list_jsonl_session_metadata, load_jsonl_session_storage,
};
use pi_agent_core::search::{
    ScanReadableQuery, ScanningReadable, ScanningReadableOptions, ScanningReadableSource, ScanningSessionSearchHit,
    ScanningSessionSearchOptions, ScanningSource, SearchError, SessionSearch, SessionSearchCandidate,
    SessionSearchOptions, create_scanning_session_search, scan_readable_entries, scanning_entries,
};
use pi_ai::types::{TextContent, TextOrImageContent, UserContent, UserMessage, UserRole};
use tempfile::TempDir;

fn message(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(text))]),
        input_source: None,
        provider_native: None,
        timestamp: 1,
    })
}

fn memory_session(id: &str) -> Session<SessionMetadata> {
    Session::new(Arc::new(InMemorySessionStorage::new(SessionMetadata {
        id: id.to_owned(),
        created_at: 1,
        parent_session_id: None,
    })))
}

fn memory_search(
    sessions: Vec<Arc<dyn ScanningReadable<SessionMetadata>>>,
) -> impl SessionSearch<ScanningSessionSearchHit> {
    create_scanning_session_search::<SessionMetadata, (), ScanningSessionSearchHit>(
        ScanningSource::Readables(sessions),
        ScanningSessionSearchOptions::default(),
    )
}

#[tokio::test]
async fn scans_an_arbitrary_in_memory_projected_source() {
    let root = memory_session("root");
    root.append_message(message("fix auth flow")).await.unwrap();
    let other = memory_session("other");
    other
        .append_message(message("auth in another workspace"))
        .await
        .unwrap();
    let search = memory_search(vec![Arc::new(root), Arc::new(other)]);
    let hits = search.search("auth", SessionSearchOptions::default()).await.unwrap();
    assert_eq!(
        hits.iter().map(|hit| hit.session_id.as_str()).collect::<Vec<_>>(),
        vec!["root", "other"]
    );
    assert!(
        search
            .search("missing", SessionSearchOptions::default())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn searches_javascript_json_for_nested_values() {
    let session = memory_session("numeric");
    let data = serde_json::from_str(r#"{"2":2.0,"1":1.0}"#).unwrap();
    session.append_custom_entry("numeric", Some(data)).await.unwrap();
    let search = memory_search(vec![Arc::new(session)]);
    let hits = search
        .search(r#""1":1,"2":2"#, SessionSearchOptions::default())
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
}

#[tokio::test]
async fn includes_labels_in_memory_scanning_projections() {
    let session = memory_session("session");
    let entry_id = session.append_message(message("plain body")).await.unwrap();
    session.set_label(&entry_id, Some("important label")).await.unwrap();
    let search = memory_search(vec![Arc::new(session)]);
    let hits = search
        .search("important", SessionSearchOptions::default())
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session_id, "session");
    assert_eq!(hits[0].entry_id, entry_id);
}

#[tokio::test]
async fn honors_entry_type_filters_limits_and_abort_signals() {
    let session = memory_session("session");
    let message_entry_id = session.append_message(message("auth message")).await.unwrap();
    session
        .append_custom_entry("note", Some(JsValue::from(serde_json::json!({"text": "auth custom"}))))
        .await
        .unwrap();
    let search = memory_search(vec![Arc::new(session)]);

    let hits = search
        .search(
            "auth",
            SessionSearchOptions {
                entry_types: Some(vec![pi_agent_core::harness::session::EntryType::Message]),
                ..SessionSearchOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].entry_id, message_entry_id);

    assert!(
        search
            .search(
                "auth",
                SessionSearchOptions {
                    entry_types: Some(Vec::new()),
                    ..SessionSearchOptions::default()
                },
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        search
            .search(
                "auth",
                SessionSearchOptions {
                    limit: Some(0),
                    ..SessionSearchOptions::default()
                },
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        search
            .search("   ", SessionSearchOptions::default())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        search
            .search(
                "auth",
                SessionSearchOptions {
                    limit: Some(1),
                    ..SessionSearchOptions::default()
                },
            )
            .await
            .unwrap()
            .len(),
        1
    );

    let token = tokio_util::sync::CancellationToken::new();
    token.cancel();
    let error = search
        .search(
            "auth",
            SessionSearchOptions {
                signal: Some(token),
                ..SessionSearchOptions::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, SearchError::Aborted));
    assert_eq!(error.to_string(), "The operation was aborted");
}

#[tokio::test]
async fn rejects_duplicate_session_ids() {
    let first = memory_session("duplicate");
    first.append_message(message("auth")).await.unwrap();
    let second = memory_session("duplicate");
    second.append_message(message("auth")).await.unwrap();
    let search = memory_search(vec![Arc::new(first), Arc::new(second)]);
    let error = search
        .search("auth", SessionSearchOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(error, SearchError::DuplicateSessionId(ref id) if id == "duplicate"));
    assert_eq!(error.to_string(), "Duplicate sessionId: duplicate");
}

#[tokio::test]
async fn supports_custom_projectors_matchers_and_hits() {
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Hit {
        session: String,
        entry: String,
        custom: bool,
    }
    impl pi_agent_core::search::SessionSearchHit for Hit {
        fn session_id(&self) -> &str {
            &self.session
        }
        fn entry_id(&self) -> &str {
            &self.entry
        }
    }
    impl From<ScanningSessionSearchHit> for Hit {
        fn from(hit: ScanningSessionSearchHit) -> Self {
            Self {
                session: hit.session_id,
                entry: hit.entry_id,
                custom: false,
            }
        }
    }

    let session = memory_session("session");
    session.append_message(message("auth body")).await.unwrap();
    let search = create_scanning_session_search::<SessionMetadata, (), Hit>(
        ScanningSource::Readables(vec![Arc::new(session)]),
        ScanningSessionSearchOptions {
            readable: ScanningReadableOptions {
                project_text: Some(Arc::new(|_metadata, entry, label| {
                    format!("{} {}", entry.entry_type().as_str(), label.unwrap_or("none"))
                })),
                page_size: Some(1),
            },
            source_options: None,
            matcher: Some(Arc::new(
                |query: &str, candidate: &SessionSearchCandidate, _metadata| candidate.text.starts_with(query),
            )),
            create_hit: Some(Arc::new(|_metadata, candidate| Hit {
                session: "session".to_owned(),
                entry: candidate.entry_id.clone(),
                custom: true,
            })),
        },
    );
    let hits = search.search("message", SessionSearchOptions::default()).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert!(hits[0].custom);
}

#[tokio::test]
async fn scans_entries_and_exposes_candidate_fields() {
    let session = Arc::new(memory_session("session"));
    let entry_id = session.append_message(message("plain body")).await.unwrap();
    session.set_label(&entry_id, Some("label")).await.unwrap();
    let candidates = scanning_entries(
        &(session.clone() as Arc<dyn ScanningReadable<SessionMetadata>>),
        &ScanningReadableOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].entry_id, entry_id);
    assert_eq!(
        candidates[0].entry_type,
        pi_agent_core::harness::session::EntryType::Message
    );
    assert!(candidates[0].text.contains("plain body"));
    assert!(candidates[0].text.contains("label"));
    assert_eq!(
        candidates[0].fields.as_ref().unwrap()["label"],
        serde_json::json!("label")
    );

    let page = scan_readable_entries(
        &(session as Arc<dyn ScanningReadable<SessionMetadata>>),
        &SessionMetadata {
            id: "session".to_owned(),
            created_at: 1,
            parent_session_id: None,
        },
        &ScanningReadableOptions {
            project_text: None,
            page_size: Some(1),
        },
        ScanReadableQuery::default(),
    )
    .await
    .unwrap();
    assert_eq!(page.len(), 1);
}

fn jsonl_source(
    options: JsonlSessionRepoOptions,
) -> ScanningReadableSource<JsonlSessionMetadata, JsonlSessionListOptions> {
    Arc::new(move |query| {
        let options = options.clone();
        let query = query.unwrap_or_default();
        Box::pin(async move {
            let mut readables: Vec<Arc<dyn ScanningReadable<JsonlSessionMetadata>>> = Vec::new();
            for metadata in list_jsonl_session_metadata(&options, &query).await? {
                let storage = load_jsonl_session_storage(&options, &metadata).await?;
                readables.push(Arc::new(Session::new(Arc::new(storage))));
            }
            Ok(readables)
        })
    })
}

#[tokio::test]
async fn scans_jsonl_sessions_from_disk_through_the_scanning_source() {
    let root = TempDir::new().unwrap();
    let options = JsonlSessionRepoOptions {
        fs: Arc::new(NativeFileSystem::new(root.path())),
        sessions_root: JsString::from(root.path().to_string_lossy().into_owned()),
    };
    let repository = JsonlSessionRepo::with_clock(options.clone(), Arc::new(|| 1_700_000_000_000));
    let cwd = root.path().join("workspace");
    let other_cwd = root.path().join("other");
    let session = repository
        .create(JsonlSessionCreateOptions {
            id: Some("jsonl".to_owned()),
            parent_session_id: None,
            cwd: JsString::from(cwd.to_string_lossy().into_owned()),
            metadata: None,
        })
        .await
        .unwrap();
    let entry_id = session
        .append_message(message("jsonl backed auth entry"))
        .await
        .unwrap();
    session.set_label(&entry_id, Some("disk label")).await.unwrap();
    let other = repository
        .create(JsonlSessionCreateOptions {
            id: Some("other".to_owned()),
            parent_session_id: None,
            cwd: JsString::from(other_cwd.to_string_lossy().into_owned()),
            metadata: None,
        })
        .await
        .unwrap();
    let other_entry_id = other
        .append_message(message("jsonl backed auth entry in another cwd"))
        .await
        .unwrap();

    let search =
        create_scanning_session_search::<JsonlSessionMetadata, JsonlSessionListOptions, ScanningSessionSearchHit>(
            ScanningSource::Dynamic(jsonl_source(options.clone())),
            ScanningSessionSearchOptions::default(),
        );
    let hits = search.search("auth", SessionSearchOptions::default()).await.unwrap();
    assert_eq!(hits.len(), 2);
    let mut entry_ids: Vec<&str> = hits.iter().map(|hit| hit.entry_id.as_str()).collect();
    entry_ids.sort_unstable();
    let mut expected = vec![entry_id.as_str(), other_entry_id.as_str()];
    expected.sort_unstable();
    assert_eq!(entry_ids, expected);

    let disk_hits = search.search("disk", SessionSearchOptions::default()).await.unwrap();
    assert_eq!(disk_hits.len(), 1);
    assert_eq!(disk_hits[0].session_id, "jsonl");
    assert_eq!(disk_hits[0].entry_id, entry_id);

    let cwd_hits = search
        .search(
            "auth",
            SessionSearchOptions {
                signal: None,
                ..SessionSearchOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(cwd_hits.len(), 2);
}

#[tokio::test]
async fn source_options_receive_the_normalized_query() {
    let session = memory_session("session");
    session.append_message(message("auth")).await.unwrap();
    let seen = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen_clone = Arc::clone(&seen);
    let search = create_scanning_session_search::<SessionMetadata, String, ScanningSessionSearchHit>(
        ScanningSource::Dynamic(Arc::new(move |options: Option<String>| {
            let sessions: Vec<Arc<dyn ScanningReadable<SessionMetadata>>> = Vec::new();
            let seen = Arc::clone(&seen_clone);
            Box::pin(async move {
                if let Some(options) = options {
                    seen.lock().unwrap().push(options);
                }
                Ok(sessions)
            })
        })),
        ScanningSessionSearchOptions {
            readable: ScanningReadableOptions::default(),
            source_options: Some(Arc::new(|text: &str, _options: &SessionSearchOptions| {
                Some(text.to_owned())
            })),
            matcher: None,
            create_hit: None,
        },
    );
    let _ = search
        .search("  AUTH  ", SessionSearchOptions::default())
        .await
        .unwrap();
    assert_eq!(seen.lock().unwrap().as_slice(), ["auth"]);
}
