use crate::shared_oracle as oracle;
use pi_coding_agent::core::session_manager::{SessionInfo, build_session_info};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

fn iso(value: f64) -> Value {
    match pi_js_compat::date::time_clip_millis(value) {
        Some(value) => json!({"ok":true,"value":pi_js_compat::date::to_iso_string(value)}),
        None => json!({"ok":false,"name":"RangeError","message":"Invalid time value"}),
    }
}
fn info(value: Option<SessionInfo>) -> Value {
    value.map_or(Value::Null,|value|json!({"id":value.id,"created":pi_js_compat::number::js_number_value(value.created),"modified":pi_js_compat::number::js_number_value(value.modified),"createdIso":iso(value.created),"modifiedIso":iso(value.modified)}))
}
fn transcript(path: &std::path::Path, header_timestamp: Value, message_timestamp: Option<Value>) {
    let mut lines=vec![json!({"type":"session","version":3,"id":path.file_stem().unwrap().to_str().unwrap(),"timestamp":header_timestamp,"cwd":"/work"}).to_string()];
    if let Some(timestamp) = message_timestamp {
        lines.push(json!({"type":"message","id":"message","parentId":null,"timestamp":"2026-01-01T00:00:00.000Z","message":{"role":"user","content":"chair","timestamp":timestamp}}).to_string());
    }
    std::fs::write(path, lines.join("\n") + "\n").unwrap();
    std::fs::File::open(path)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_millis(2_000_000_000))
        .unwrap();
}

fn time_value(value: &str) -> f64 {
    pi_js_compat::date::time_clip_millis(value.parse().unwrap()).map_or(f64::NAN, |value| value as f64)
}
fn row_info(row: &Value) -> SessionInfo {
    SessionInfo {
        path: format!("/tmp/{}.jsonl", row["id"].as_str().unwrap()).into(),
        id: row["id"].as_str().unwrap().into(),
        cwd: "/work".into(),
        name: row["name"].as_str().map(Into::into),
        is_archived: row["isArchived"].as_bool(),
        parent_session_path: row["parentSessionPath"].as_str().map(Into::into),
        created: time_value(row["created"].as_str().unwrap()),
        modified: time_value(row["modified"].as_str().unwrap()),
        message_count: 1,
        first_message: "chair".into(),
    }
}
fn rows(length: usize) -> Vec<Value> {
    (0..length).map(|index|json!({"id":format!("s{index:03}"),"created":"0","modified":if index%7==2 {"NaN".to_owned()}else{((index*37)%100).to_string()}})).collect()
}
struct Loader(Vec<SessionInfo>);
impl pi_coding_agent::cli::session_picker::SessionLoader for Loader {
    fn load<'a>(
        &'a self,
        _progress: Option<pi_coding_agent::core::session_manager::SessionListProgress<'a>>,
    ) -> pi_coding_agent::cli::session_picker::SessionListFuture<'a, Result<Vec<SessionInfo>, JsString>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

#[tokio::test(flavor = "current_thread")]
async fn picker_date_sort_matches_frozen_with_invalid_dates() {
    let mut oracle = oracle::TsOracle::spawn("session-info-dates-oracle.ts", "packages/coding-agent/src");
    for length in [7, 17, 32, 70, 129] {
        let rows = rows(length);
        let response = oracle.request(&json!({"id":2,"operation":"picker","rows":rows,"archived":false}));
        assert_eq!(response["ok"], true, "{response}");
        let loader = Loader(rows.iter().map(row_info).collect());
        let actual = pi_coding_agent::cli::session_picker::load_sessions(&loader, None, false)
            .await
            .unwrap();
        assert_eq!(
            json!(actual.into_iter().map(|session| session.id).collect::<Vec<_>>()),
            response["output"],
            "length={length}"
        );
    }
}

#[test]
fn search_date_ties_match_frozen_with_invalid_dates() {
    use pi_coding_agent::modes::interactive::components::session_selector_search::{
        NameFilter, SortMode, filter_and_sort_sessions,
    };
    let mut oracle = oracle::TsOracle::spawn("session-info-dates-oracle.ts", "packages/coding-agent/src");
    for length in [7, 17, 32, 70, 129] {
        let rows = rows(length);
        let response = oracle.request(&json!({"id":3,"operation":"search","rows":rows,"query":"\"chair\""}));
        assert_eq!(response["ok"], true, "{response}");
        let sessions: Vec<_> = rows.iter().map(row_info).collect();
        let actual: Vec<Vec<JsString>> = [SortMode::Recent, SortMode::Relevance, SortMode::Threaded]
            .into_iter()
            .map(|sort| {
                filter_and_sort_sessions(&sessions, &"\"chair\"".into(), sort, NameFilter::All)
                    .into_iter()
                    .map(|session| session.id)
                    .collect()
            })
            .collect();
        assert_eq!(json!(actual), response["output"], "length={length}");
    }
}

#[test]
fn selector_invalid_date_render_and_subtree_order_match_frozen() {
    use pi_coding_agent::{
        core::keybindings::KeybindingsManager,
        modes::interactive::components::{
            session_selector::SessionList,
            session_selector_search::{NameFilter, SortMode},
        },
    };
    use pi_tui::Component;
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
    let mut oracle = oracle::TsOracle::spawn("session-info-dates-oracle.ts", "packages/coding-agent/src");
    let rows = json!([{"id":"root","created":"NaN","modified":"8640000000000000"},{"id":"child","created":"Infinity","modified":"NaN","parentSessionPath":"/tmp/root.jsonl"},{"id":"other","created":"1.9","modified":"8640000000000000"}]);
    for (sort, native_sort) in [("recent", SortMode::Recent), ("threaded", SortMode::Threaded)] {
        let response = oracle.request(&json!({"id":4,"operation":"selector","rows":rows,"sort":sort,"width":100}));
        assert_eq!(response["ok"], true, "{response}");
        let mut list = SessionList::new(
            Vec::new(),
            false,
            SortMode::Recent,
            NameFilter::All,
            KeybindingsManager::new(Default::default(), None),
            None,
        );
        list.set_sessions(rows.as_array().unwrap().iter().map(row_info).collect(), false);
        list.set_sort_mode(native_sort);
        assert_eq!(
            json!({"frame":list.render(100),"selected":list.get_selected_session_path()}),
            response["output"],
            "sort={sort}"
        );
    }
}

#[test]
fn metadata_loading_and_cache_iso_failure_match_frozen() {
    use pi_coding_agent::core::{
        session_control_db::{WritableSessionMetadata, write_session_metadata},
        session_manager::SessionManager,
    };
    let mut oracle = oracle::TsOracle::spawn("session-info-dates-oracle.ts", "packages/coding-agent/src");
    let directory = tempfile::tempdir().unwrap();
    let dir = JsString::from(directory.path().display().to_string());
    let db = JsString::from(directory.path().join("control.sqlite").display().to_string());
    let metadata:WritableSessionMetadata=serde_json::from_value(json!({"sessionPath":directory.path().join("metadata.jsonl").display().to_string(),"id":"metadata","cwd":"/work","createdAt":"not a date","modifiedAt":"not a date","messageCount":1,"firstMessage":"chair"})).unwrap();
    write_session_metadata(&db, &metadata).unwrap();
    let response = oracle.request(&json!({"id":5,"operation":"list","dir":dir,"db":db}));
    assert_eq!(response["ok"], true, "{response}");
    let actual: Vec<_> = SessionManager::list_all(Some(&dir), None, Some(&db))
        .unwrap()
        .into_iter()
        .map(|value| info(Some(value)))
        .collect();
    assert_eq!(json!(actual), response["output"]);

    let empty = tempfile::tempdir().unwrap();
    transcript(&empty.path().join("invalid.jsonl"), json!("not a date"), None);
    let dir = JsString::from(empty.path().display().to_string());
    let db = JsString::from(empty.path().join("control.sqlite").display().to_string());
    let response = oracle.request(&json!({"id":6,"operation":"list","dir":dir,"db":db}));
    assert_eq!(response["ok"], false, "{response}");
    let error = SessionManager::list_all(Some(&dir), None, Some(&db)).unwrap_err();
    assert_eq!(
        json!({"name":error.name(),"message":error.message()}),
        json!({"name":response["name"],"message":response["message"]})
    );
}

#[test]
fn append_metadata_keeps_numbers_until_the_frozen_iso_failure_boundary() {
    use pi_agent_core::agent::AgentMessage;
    use pi_ai::types::{UserContent, UserMessage, UserRole};
    use pi_coding_agent::core::{
        session_control_db::read_session_metadata,
        session_manager::{SessionManager, SetMetadataControlDbPathOptions},
    };
    let mut oracle = oracle::TsOracle::spawn("session-info-dates-oracle.ts", "packages/coding-agent/src");
    for timestamp in ["NaN", "Infinity", "-Infinity", "100000000000000000000", "12.75"] {
        let source = tempfile::tempdir().unwrap();
        let response=oracle.request(&json!({"id":9,"operation":"appendMetadata","dir":source.path(),"db":source.path().join("control.sqlite"),"timestamp":timestamp}));
        assert_eq!(response["ok"], true, "{response}");
        let native = tempfile::tempdir().unwrap();
        let dir = JsString::from(native.path().display().to_string());
        let db = JsString::from(native.path().join("control.sqlite").display().to_string());
        let mut manager = SessionManager::create(&"/work".into(), Some(&dir), None).unwrap();
        manager
            .set_metadata_control_db_path(Some(&db), SetMetadataControlDbPathOptions::default())
            .unwrap();
        let result = manager.append_message(AgentMessage::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text("chair".into()),
            input_source: None,
            provider_native: None,
            timestamp: timestamp.parse().unwrap(),
        }));
        let outcome = match result {
            Ok(_) => json!({"ok":true}),
            Err(error) => json!({"ok":false,"name":error.name(),"message":error.message()}),
        };
        let metadata = read_session_metadata(&db, manager.get_session_file().unwrap())
            .unwrap()
            .unwrap();
        let modified = if metadata.created_at == metadata.modified_at {
            "<header>".to_owned()
        } else {
            metadata.modified_at
        };
        assert_eq!(
            json!({"outcome":outcome,"entries":manager.get_entries().len(),"messageCount":metadata.message_count,"firstMessage":metadata.first_message,"modified":modified}),
            response["output"],
            "timestamp={timestamp}"
        );
    }
}

#[test]
fn filesystem_session_order_preserves_frozen_invalid_date_comparisons() {
    use pi_coding_agent::core::session_manager::SessionManager;
    let mut oracle = oracle::TsOracle::spawn("session-info-dates-oracle.ts", "packages/coding-agent/src");
    for length in [7, 70] {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..length {
            let timestamp = if index % 7 == 2 {
                1e20
            } else {
                1000.0 + ((index * 37) % 100) as f64
            };
            transcript(
                &directory.path().join(format!("file{index:03}.jsonl")),
                json!("2026-01-01T00:00:00.000Z"),
                Some(json!(timestamp)),
            );
        }
        let dir = JsString::from(directory.path().display().to_string());
        let response = oracle.request(&json!({"id":8,"operation":"list","dir":dir}));
        assert_eq!(response["ok"], true, "{response}");
        let actual: Vec<_> = SessionManager::list_all(Some(&dir), None, None)
            .unwrap()
            .into_iter()
            .map(|value| info(Some(value)))
            .collect();
        assert_eq!(json!(actual), response["output"], "length={length}");
    }
}

#[test]
fn file_session_info_preserves_invalid_dates_and_timeclip() {
    let mut oracle = oracle::TsOracle::spawn("session-info-dates-oracle.ts", "packages/coding-agent/src");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fixture.jsonl");
    let native_path = JsString::from(path.display().to_string());
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for (header, message) in [
        (json!("not a date"), None),
        (json!("2026-01-01T00:00:00.000Z"), Some(json!(1e20))),
        (json!("2026-01-01T00:00:00.000Z"), Some(json!(1000.9))),
        (json!(1.9), None),
        (json!("2026-01-01T00:00:00.000Z"), Some(json!(-1.9))),
    ] {
        transcript(&path, header, message);
        let response = oracle.request(&json!({"id":1,"operation":"build","path":native_path}));
        assert_eq!(response["ok"], true, "{response}");
        expected.push(response["output"].clone());
        actual.push(info(build_session_info(&native_path)));
    }
    transcript(&path, json!("not a date"), None);
    std::fs::File::open(&path)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH - std::time::Duration::from_millis(1000))
        .unwrap();
    let response = oracle.request(&json!({"id":1,"operation":"build","path":native_path}));
    assert_eq!(response["ok"], true, "{response}");
    expected.push(response["output"].clone());
    actual.push(info(build_session_info(&native_path)));
    assert_eq!(actual, expected);
}
