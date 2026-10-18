#[path = "differential_session/support.rs"]
mod support;

use std::collections::HashMap;
use std::path::Path;

use pi_agent_core::harness::session::jsonl::codec::{encode_header, encode_mutation, parse_header, parse_mutation};
use pi_agent_core::harness::session::jsonl::types::NativeFileSystem;
use pi_agent_core::harness::session::{
    BranchQuery, EntryQuery, ForkOptions, JsonlSessionCreateOptions, JsonlSessionListOptions, JsonlSessionMetadata,
    JsonlSessionRepo, JsonlSessionRepoOptions, JsonlV4Header, LogOptions, RecordQuery, Session,
    SessionContextBuildOptions, SessionCreateOptions, SessionMetadata, build_session_context,
};
use pi_agent_core::search::{
    ScanningReadable, ScanningSessionSearchHit, ScanningSessionSearchOptions, ScanningSource, SessionSearch,
    SessionSearchOptions, create_scanning_session_search,
};
use serde_json::{Value, json};
use support::{
    TsOracle, branch_query, compare, entries_json, entry_json, entry_query, error_json, expect_ok, fixture_entry,
    fixture_record, normalize_jsonl_metadata, normalize_session_metadata, oracle_request, read_files, record_json,
    record_query, records_json,
};
use tempfile::TempDir;

const ORACLE: &str = "session-oracle.ts";
const SOURCE_MODULE: &str = "packages/agent/src/harness/session/index.ts";

fn jsonl_repo(root: &Path) -> JsonlSessionRepo {
    JsonlSessionRepo::with_clock(
        JsonlSessionRepoOptions {
            fs: std::sync::Arc::new(NativeFileSystem::new(root)),
            sessions_root: root.to_string_lossy().into_owned(),
        },
        std::sync::Arc::new(|| 1_700_000_000_000),
    )
}

fn flatten_mutation(mutation: &Value) -> Value {
    let mut object = mutation.as_object().cloned().unwrap_or_default();
    if object.get("kind").and_then(Value::as_str) == Some("entry")
        && let Some(entry) = object.remove("entry")
    {
        for (key, value) in entry.as_object().cloned().unwrap_or_default() {
            object.insert(key, value);
        }
    }
    if object.get("kind").and_then(Value::as_str) == Some("record")
        && let Some(record) = object.remove("record")
    {
        for (key, value) in record.as_object().cloned().unwrap_or_default() {
            object.insert(key, value);
        }
    }
    Value::Object(object)
}

fn compare_error(
    id: u64,
    request: Value,
    error: &pi_agent_core::harness::session::SessionError,
    oracle: &mut TsOracle,
) {
    let response = oracle.request(&request);
    assert_eq!(
        response["ok"],
        json!(false),
        "oracle unexpectedly succeeded: {response}"
    );
    assert_eq!(
        &response["error"],
        &error_json(error),
        "differential error mismatch for {request}"
    );
    let _ = id;
}

#[test]
fn codec_matches_the_pinned_typescript_source() {
    let mut oracle = TsOracle::spawn(ORACLE, SOURCE_MODULE);
    let headers = vec![
        json!({
            "kind": "header",
            "version": 4,
            "id": "session",
            "createdAt": 1_700_000_000_000_i64,
            "cwd": "/workspace/project",
            "parentSessionId": "parent",
            "metadata": {"owner": "agent", "nested": {"enabled": true}, "values": [1, null, "two"]},
        }),
        json!({
            "kind": "header",
            "version": 4,
            "id": "legacy-child",
            "createdAt": 1_700_000_000_001_i64,
            "cwd": "/workspace/project",
            "legacyParentSessionPath": "/sessions/missing-parent.jsonl",
        }),
    ];
    let mutations = vec![
        json!({"kind": "entry", "lane": "main", "entry": {"type": "custom", "id": "entry-1", "customType": "note", "data": {"text": "hello"}, "parentId": null, "seq": 1, "timestamp": 100}}),
        json!({"kind": "entry", "entry": {"type": "custom", "id": "entry-1", "customType": "note", "parentId": null, "seq": 1, "timestamp": 100}}),
        json!({"kind": "record", "record": {"type": "operation_started", "id": "run-1", "lane": "main", "sourceLeafId": null, "intent": {"kind": "run", "originalPrompt": [], "initialMessages": []}, "seq": 1, "timestamp": 100}}),
        json!({"kind": "lane", "seq": 1, "lane": "thread", "leafId": "entry-1"}),
        json!({"kind": "fact", "seq": 1, "fact": "name", "name": "Example"}),
        json!({"kind": "fact", "seq": 2, "fact": "label", "targetId": "entry-1", "label": "checkpoint"}),
    ];
    let mutation_lines = vec![
        "{".to_owned(),
        json!({"kind": "unknown", "seq": 1}).to_string(),
        json!({"kind": "entry", "type": "custom", "id": "entry", "parentId": null, "seq": 1, "timestamp": 1})
            .to_string(),
    ];
    let request = oracle_request(
        1,
        "codec",
        json!({
            "headers": headers,
            "mutations": mutations,
            "mutationLines": mutation_lines,
        }),
    );
    let response = oracle.request(&request);
    let output = expect_ok(&response);

    let encoded_headers: Vec<Value> = headers
        .iter()
        .map(|header| {
            let header: JsonlV4Header = serde_json::from_value(header.clone()).unwrap();
            json!(encode_header(&header))
        })
        .collect();
    let parsed_headers: Vec<Value> = headers
        .iter()
        .map(|header| match parse_header(&header.to_string()) {
            Ok(value) => json!({"ok": true, "value": value}),
            Err(error) => json!({"ok": false, "error": {"name": "JsonlDecodeError", "kind": error.kind.as_str()}}),
        })
        .collect();
    let encoded_mutations: Vec<Value> = mutations
        .iter()
        .map(|mutation| {
            let flattened = flatten_mutation(mutation);
            let parsed = parse_mutation(&flattened.to_string()).unwrap();
            json!(encode_mutation(&parsed).unwrap())
        })
        .collect();
    let parsed_mutations: Vec<Value> = mutation_lines
        .iter()
        .map(|line| match parse_mutation(line) {
            Ok(value) => json!({"ok": true, "value": value}),
            Err(error) => json!({"ok": false, "error": {"name": "JsonlDecodeError", "kind": error.kind.as_str()}}),
        })
        .collect();
    assert_eq!(
        output,
        &json!({
            "encodedHeaders": encoded_headers,
            "parsedHeaders": parsed_headers,
            "encodedMutations": encoded_mutations,
            "parsedMutations": parsed_mutations,
        })
    );
}

#[tokio::test]
async fn jsonl_session_operations_match_the_pinned_typescript_source() {
    let ts_root = TempDir::new().unwrap();
    let rust_root = TempDir::new().unwrap();
    let mut oracle = TsOracle::spawn(ORACLE, SOURCE_MODULE);
    let repo = jsonl_repo(rust_root.path());
    let mut sessions: HashMap<String, Session<JsonlSessionMetadata>> = HashMap::new();
    let mut metadata: HashMap<String, JsonlSessionMetadata> = HashMap::new();
    let mut next_id = 10_u64;

    expect_ok(&oracle.request(&oracle_request(
        1,
        "reset",
        json!({"root": ts_root.path(), "mode": "jsonl", "clock": 1_700_000_000_000_i64}),
    )));

    let create = oracle_request(
        2,
        "create",
        json!({"key": "s", "sessionId": "session", "cwd": "workspace", "metadata": {"owner": "agent", "nested": {"enabled": true}}}),
    );
    let created = repo
        .create(JsonlSessionCreateOptions {
            id: Some("session".to_owned()),
            parent_session_id: None,
            cwd: rust_root.path().join("workspace").to_string_lossy().into_owned(),
            metadata: Some(
                json!({"owner": "agent", "nested": {"enabled": true}})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        })
        .await
        .unwrap();
    let created_metadata = created.get_metadata().await.unwrap();
    sessions.insert("s".to_owned(), created);
    metadata.insert("s".to_owned(), created_metadata.clone());
    compare(
        2,
        "create",
        create,
        normalize_jsonl_metadata(&created_metadata, &rust_root.path().to_string_lossy()),
        &mut oracle,
    );

    let entries = vec![
        json!({"type": "message", "id": "m1", "message": {"role": "user", "content": [{"type": "text", "text": "hello auth"}], "timestamp": 1}}),
        json!({"type": "custom", "id": "n1", "customType": "note", "data": {"text": "auth note"}}),
        json!({"type": "model_change", "id": "model", "provider": "openai", "modelId": "gpt-5"}),
        json!({"type": "thinking_level_change", "id": "thinking", "thinkingLevel": "high"}),
        json!({"type": "active_tools_change", "id": "tools", "activeToolNames": ["read", "bash"]}),
        json!({"type": "compaction", "id": "compact", "summary": "summary", "tokensBefore": 100, "retainedTail": [{"role": "user", "content": [{"type": "text", "text": "retained"}], "timestamp": 1}], "details": {"source": "test"}, "usage": {"input": 1, "output": 2, "cacheRead": 3, "cacheWrite": 4, "totalTokens": 10, "cost": {"input": 0.1, "output": 0.2, "cacheRead": 0.3, "cacheWrite": 0.4, "total": 1}}}),
        json!({"type": "branch_summary", "id": "branch", "fromId": "m1", "summary": "branch summary", "details": null}),
    ];
    for entry in &entries {
        let id = next_id;
        next_id += 1;
        let request = oracle_request(id, "append_entry", json!({"key": "s", "lane": "main", "entry": entry}));
        let committed = sessions["s"]
            .append_entry(fixture_entry(entry.clone()), "main")
            .await
            .unwrap();
        compare(id, "append_entry", request, entry_json(&committed), &mut oracle);
    }

    let lane_request = oracle_request(11, "create_lane", json!({"key": "s", "lane": "thread", "at": "m1"}));
    sessions["s"].create_lane("thread", Some("m1")).await.unwrap();
    compare(11, "create_lane", lane_request, Value::Null, &mut oracle);

    let record = json!({"type": "operation_started", "id": "run", "lane": "thread", "sourceLeafId": "m1", "intent": {"kind": "run", "originalPrompt": [{"role": "user", "content": [{"type": "text", "text": "prompt"}], "timestamp": 1}], "initialMessages": [], "systemPromptOverride": "system", "resumeData": {"extension": {"version": 1}}}});
    let request = oracle_request(12, "append_record", json!({"key": "s", "record": record}));
    let committed = sessions["s"].append_record(fixture_record(record)).await.unwrap();
    compare(12, "append_record", request, record_json(&committed), &mut oracle);

    let usage = json!({"type": "usage", "id": "usage", "lane": "main", "cause": "assistant", "runId": "run", "entryId": "m1", "attempt": 1, "stopReason": "stop", "usage": {"input": 10, "output": 5, "cacheRead": 3, "cacheWrite": 2, "totalTokens": 20, "cost": {"input": 1, "output": 2, "cacheRead": 3, "cacheWrite": 4, "total": 10}}});
    let request = oracle_request(13, "append_record", json!({"key": "s", "record": usage}));
    let committed = sessions["s"].append_record(fixture_record(usage)).await.unwrap();
    compare(13, "append_record", request, record_json(&committed), &mut oracle);

    let name_request = oracle_request(14, "set_name", json!({"key": "s", "name": "Example"}));
    sessions["s"].set_name(Some("Example")).await.unwrap();
    compare(14, "set_name", name_request, Value::Null, &mut oracle);

    let label_request = oracle_request(
        15,
        "set_label",
        json!({"key": "s", "entryId": "n1", "label": "checkpoint"}),
    );
    sessions["s"].set_label("n1", Some("checkpoint")).await.unwrap();
    compare(15, "set_label", label_request, Value::Null, &mut oracle);

    let move_request = oracle_request(16, "move_lane", json!({"key": "s", "lane": "main", "to": "n1"}));
    sessions["s"].move_lane("main", Some("n1")).await.unwrap();
    compare(16, "move_lane", move_request, Value::Null, &mut oracle);

    let query = json!({"order": "oldestFirst"});
    let request = oracle_request(17, "find_entries", json!({"key": "s", "query": query}));
    let entries = sessions["s"].find_entries(entry_query(&query)).await.unwrap();
    compare(17, "find_entries", request, entries_json(&entries), &mut oracle);

    let query = json!({"customType": "note"});
    let request = oracle_request(18, "find_entries", json!({"key": "s", "query": query}));
    let entries = sessions["s"].find_entries(entry_query(&query)).await.unwrap();
    compare(18, "find_entries", request, entries_json(&entries), &mut oracle);

    let query = json!({"start": "compact", "stopAtType": "message", "order": "oldestFirst"});
    let request = oracle_request(19, "find_entries_on_branch", json!({"key": "s", "query": query}));
    let entries = sessions["s"]
        .find_entries_on_branch(branch_query(&query))
        .await
        .unwrap();
    compare(
        19,
        "find_entries_on_branch",
        request,
        entries_json(&entries),
        &mut oracle,
    );

    let query = json!({"type": "usage"});
    let request = oracle_request(20, "find_records", json!({"key": "s", "query": query}));
    let records = sessions["s"].find_records(record_query(&query)).await.unwrap();
    compare(20, "find_records", request, records_json(&records), &mut oracle);

    let query = json!({"type": "operation_started", "operationKind": "run"});
    let request = oracle_request(21, "find_records", json!({"key": "s", "query": query}));
    let records = sessions["s"].find_records(record_query(&query)).await.unwrap();
    compare(21, "find_records", request, records_json(&records), &mut oracle);

    let request = oracle_request(
        22,
        "find_open_operations",
        json!({"key": "s", "lane": "thread", "limit": 2}),
    );
    let records = sessions["s"].find_open_operations("thread", Some(2)).await.unwrap();
    compare(22, "find_open_operations", request, records_json(&records), &mut oracle);

    let request = oracle_request(
        23,
        "get_log",
        json!({"key": "s", "options": {"afterSeq": 2, "limit": 3}}),
    );
    let log = sessions["s"]
        .get_log(LogOptions {
            after_seq: Some(2),
            limit: Some(3),
        })
        .await
        .unwrap();
    compare(23, "get_log", request, serde_json::to_value(log).unwrap(), &mut oracle);

    let request = oracle_request(24, "get_lanes", json!({"key": "s"}));
    let lanes = sessions["s"].get_lanes().await.unwrap();
    compare(
        24,
        "get_lanes",
        request,
        serde_json::to_value(lanes).unwrap(),
        &mut oracle,
    );

    let request = oracle_request(25, "get_stats", json!({"key": "s"}));
    let stats = sessions["s"].get_stats().await.unwrap();
    compare(
        25,
        "get_stats",
        request,
        serde_json::to_value(stats).unwrap(),
        &mut oracle,
    );

    let request = oracle_request(26, "get_name", json!({"key": "s"}));
    let name = sessions["s"].get_name().await.unwrap();
    compare(26, "get_name", request, json!(name), &mut oracle);

    let request = oracle_request(27, "get_label", json!({"key": "s", "entryId": "n1"}));
    let label = sessions["s"].get_label("n1").await.unwrap();
    compare(27, "get_label", request, json!(label), &mut oracle);

    let request = oracle_request(28, "get_leaf_id", json!({"key": "s"}));
    let leaf = sessions["s"].get_leaf_id().await.unwrap();
    compare(28, "get_leaf_id", request, json!(leaf), &mut oracle);

    let request = oracle_request(29, "get_metadata", json!({"key": "s"}));
    let current = sessions["s"].get_metadata().await.unwrap();
    compare(
        29,
        "get_metadata",
        request,
        normalize_jsonl_metadata(&current, &rust_root.path().to_string_lossy()),
        &mut oracle,
    );

    let request = oracle_request(30, "list", json!({}));
    let listed = repo.list(JsonlSessionListOptions::default()).await.unwrap();
    let rust_list: Vec<Value> = listed
        .iter()
        .map(|metadata| normalize_jsonl_metadata(metadata, &rust_root.path().to_string_lossy()))
        .collect();
    compare(30, "list", request, Value::Array(rust_list), &mut oracle);

    let request = oracle_request(31, "files", json!({}));
    compare(
        31,
        "files",
        request,
        Value::Object(read_files(rust_root.path())),
        &mut oracle,
    );

    let fork_request = oracle_request(
        32,
        "fork",
        json!({"key": "tree", "sourceKey": "s", "sessionId": "tree-fork", "cwd": "workspace", "options": {"scope": "tree"}}),
    );
    let tree = repo
        .fork(
            &metadata["s"],
            ForkOptions::Tree,
            JsonlSessionCreateOptions {
                id: Some("tree-fork".to_owned()),
                parent_session_id: None,
                cwd: rust_root.path().join("workspace").to_string_lossy().into_owned(),
                metadata: None,
            },
        )
        .await
        .unwrap();
    let tree_metadata = tree.get_metadata().await.unwrap();
    sessions.insert("tree".to_owned(), tree);
    metadata.insert("tree".to_owned(), tree_metadata.clone());
    compare(
        32,
        "fork",
        fork_request,
        normalize_jsonl_metadata(&tree_metadata, &rust_root.path().to_string_lossy()),
        &mut oracle,
    );

    let branch_request = oracle_request(
        33,
        "fork",
        json!({"key": "branch", "sourceKey": "s", "sessionId": "branch-fork", "cwd": "workspace", "options": {"scope": "branch", "entryId": "m1", "position": "at"}}),
    );
    let branch = repo
        .fork(
            &metadata["s"],
            ForkOptions::BranchAt {
                entry_id: "m1".to_owned(),
            },
            JsonlSessionCreateOptions {
                id: Some("branch-fork".to_owned()),
                parent_session_id: None,
                cwd: rust_root.path().join("workspace").to_string_lossy().into_owned(),
                metadata: None,
            },
        )
        .await
        .unwrap();
    let branch_metadata = branch.get_metadata().await.unwrap();
    sessions.insert("branch".to_owned(), branch);
    metadata.insert("branch".to_owned(), branch_metadata.clone());
    compare(
        33,
        "fork",
        branch_request,
        normalize_jsonl_metadata(&branch_metadata, &rust_root.path().to_string_lossy()),
        &mut oracle,
    );

    let open_request = oracle_request(34, "open", json!({"key": "reopened", "sourceKey": "tree"}));
    let reopened = repo.open(&tree_metadata).await.unwrap();
    let reopened_metadata = reopened.get_metadata().await.unwrap();
    sessions.insert("reopened".to_owned(), reopened);
    compare(
        34,
        "open",
        open_request,
        normalize_jsonl_metadata(&reopened_metadata, &rust_root.path().to_string_lossy()),
        &mut oracle,
    );

    let request = oracle_request(
        35,
        "find_entries",
        json!({"key": "reopened", "query": {"order": "oldestFirst"}}),
    );
    let entries = sessions["reopened"]
        .find_entries(EntryQuery {
            order: Some(pi_agent_core::harness::session::EntryOrder::OldestFirst),
            ..EntryQuery::default()
        })
        .await
        .unwrap();
    compare(35, "find_entries", request, entries_json(&entries), &mut oracle);

    let request = oracle_request(36, "files", json!({}));
    compare(
        36,
        "files",
        request,
        Value::Object(read_files(rust_root.path())),
        &mut oracle,
    );

    let delete_request = oracle_request(37, "delete", json!({"key": "branch"}));
    repo.delete(&branch_metadata).await.unwrap();
    compare(37, "delete", delete_request, Value::Null, &mut oracle);

    let list_request = oracle_request(38, "list", json!({}));
    let listed = repo.list(JsonlSessionListOptions::default()).await.unwrap();
    let rust_list: Vec<Value> = listed
        .iter()
        .map(|metadata| normalize_jsonl_metadata(metadata, &rust_root.path().to_string_lossy()))
        .collect();
    compare(38, "list", list_request, Value::Array(rust_list), &mut oracle);

    let open_request = oracle_request(39, "open", json!({"key": "missing", "sourceKey": "branch"}));
    let error = repo.open(&branch_metadata).await.unwrap_err();
    compare_error(39, open_request, &error, &mut oracle);
}

#[tokio::test]
async fn jsonl_cross_reads_match_in_both_directions() {
    let root = TempDir::new().unwrap();
    let mut oracle = TsOracle::spawn(ORACLE, SOURCE_MODULE);
    let repo = jsonl_repo(root.path());

    let reset = oracle_request(
        1,
        "reset",
        json!({"root": root.path(), "mode": "jsonl", "clock": 1_700_000_000_000_i64}),
    );
    expect_ok(&oracle.request(&reset));

    let create = oracle_request(2, "create", json!({"key": "s", "sessionId": "cross", "cwd": "."}));
    let response = oracle.request(&create);
    expect_ok(&response);
    for (id, entry) in [
        (
            3,
            json!({"type": "message", "id": "ts-1", "message": {"role": "user", "content": [{"type": "text", "text": "from ts"}], "timestamp": 1}}),
        ),
        (
            4,
            json!({"type": "custom", "id": "ts-2", "customType": "note", "data": {"from": "ts"}}),
        ),
    ] {
        let request = oracle_request(id, "append_entry", json!({"key": "s", "lane": "main", "entry": entry}));
        let response = oracle.request(&request);
        expect_ok(&response);
    }

    let listed = repo.list(JsonlSessionListOptions::default()).await.unwrap();
    assert_eq!(listed.len(), 1);
    let metadata = listed[0].clone();
    let rust_session = repo.open(&metadata).await.unwrap();
    let rust_entry = rust_session
        .append_entry(
            fixture_entry(json!({"type": "custom", "id": "rust-3", "customType": "note", "data": {"from": "rust"}})),
            "main",
        )
        .await
        .unwrap();
    let rust_log = rust_session.get_log(LogOptions::default()).await.unwrap();
    let rust_log_json = serde_json::to_value(&rust_log).unwrap();
    assert_eq!(
        rust_log_json.as_array().unwrap().len(),
        3,
        "header mutations: {rust_entry:?}"
    );

    let open = oracle_request(5, "open", json!({"key": "reopened", "sourceKey": "s"}));
    let response = oracle.request(&open);
    expect_ok(&response);
    let request = oracle_request(6, "get_log", json!({"key": "reopened", "options": {}}));
    let response = oracle.request(&request);
    let oracle_log = expect_ok(&response).clone();
    assert_eq!(oracle_log, rust_log_json);

    let ts_append = oracle_request(
        7,
        "append_entry",
        json!({"key": "reopened", "lane": "main", "entry": {"type": "custom", "id": "ts-4", "customType": "note", "data": {"from": "ts-again"}}}),
    );
    let response = oracle.request(&ts_append);
    expect_ok(&response);

    let reopened = repo.open(&metadata).await.unwrap();
    let rust_log = reopened.get_log(LogOptions::default()).await.unwrap();
    let request = oracle_request(8, "get_log", json!({"key": "reopened", "options": {}}));
    let response = oracle.request(&request);
    assert_eq!(expect_ok(&response), &serde_json::to_value(&rust_log).unwrap());

    let request = oracle_request(9, "files", json!({}));
    let response = oracle.request(&request);
    assert_eq!(expect_ok(&response), &Value::Object(read_files(root.path())));
}

#[tokio::test]
async fn memory_session_operations_match_the_pinned_typescript_source() {
    let mut oracle = TsOracle::spawn(ORACLE, SOURCE_MODULE);
    let repo =
        pi_agent_core::harness::session::InMemorySessionRepo::with_clock(std::sync::Arc::new(|| 1_700_000_000_000));
    let mut sessions: HashMap<String, Session<SessionMetadata>> = HashMap::new();
    let mut metadata: HashMap<String, SessionMetadata> = HashMap::new();

    expect_ok(&oracle.request(&oracle_request(
        1,
        "reset",
        json!({"root": "/tmp/memory", "mode": "memory", "clock": 1_700_000_000_000_i64}),
    )));

    let create = oracle_request(2, "create", json!({"key": "s", "sessionId": "session"}));
    let session = repo
        .create(SessionCreateOptions {
            id: Some("session".to_owned()),
            parent_session_id: None,
        })
        .await
        .unwrap();
    let current = session.get_metadata().await.unwrap();
    sessions.insert("s".to_owned(), session);
    metadata.insert("s".to_owned(), current.clone());
    compare(
        2,
        "create",
        create,
        normalize_session_metadata(&current, ""),
        &mut oracle,
    );

    for (id, entry) in [
        (
            3,
            json!({"type": "message", "id": "m1", "message": {"role": "user", "content": [{"type": "text", "text": "hello auth"}], "timestamp": 1}}),
        ),
        (
            4,
            json!({"type": "custom", "id": "n1", "customType": "note", "data": {"text": "auth note"}}),
        ),
        (
            5,
            json!({"type": "compaction", "id": "compact", "summary": "summary", "tokensBefore": 100, "retainedTail": []}),
        ),
    ] {
        let request = oracle_request(id, "append_entry", json!({"key": "s", "lane": "main", "entry": entry}));
        let committed = sessions["s"].append_entry(fixture_entry(entry), "main").await.unwrap();
        compare(id, "append_entry", request, entry_json(&committed), &mut oracle);
    }

    let record = json!({"type": "usage", "id": "usage", "lane": "main", "cause": "assistant", "runId": "run", "entryId": "m1", "attempt": 1, "stopReason": "stop", "usage": {"input": 10, "output": 5, "cacheRead": 3, "cacheWrite": 2, "totalTokens": 20, "cost": {"input": 1, "output": 2, "cacheRead": 3, "cacheWrite": 4, "total": 10}}});
    let request = oracle_request(6, "append_record", json!({"key": "s", "record": record}));
    let committed = sessions["s"].append_record(fixture_record(record)).await.unwrap();
    compare(6, "append_record", request, record_json(&committed), &mut oracle);

    let label = oracle_request(
        7,
        "set_label",
        json!({"key": "s", "entryId": "n1", "label": "important"}),
    );
    sessions["s"].set_label("n1", Some("important")).await.unwrap();
    compare(7, "set_label", label, Value::Null, &mut oracle);

    let query = json!({"order": "oldestFirst"});
    let request = oracle_request(8, "find_entries", json!({"key": "s", "query": query}));
    let entries = sessions["s"].find_entries(entry_query(&query)).await.unwrap();
    compare(8, "find_entries", request, entries_json(&entries), &mut oracle);

    let request = oracle_request(9, "get_stats", json!({"key": "s"}));
    let stats = sessions["s"].get_stats().await.unwrap();
    compare(
        9,
        "get_stats",
        request,
        serde_json::to_value(stats).unwrap(),
        &mut oracle,
    );

    let fork = oracle_request(
        10,
        "fork",
        json!({"key": "fork", "sourceKey": "s", "sessionId": "fork", "options": {"scope": "tree"}}),
    );
    let forked = repo
        .fork(
            &metadata["s"],
            ForkOptions::Tree,
            SessionCreateOptions {
                id: Some("fork".to_owned()),
                parent_session_id: None,
            },
        )
        .await
        .unwrap();
    let forked_metadata = forked.get_metadata().await.unwrap();
    sessions.insert("fork".to_owned(), forked);
    metadata.insert("fork".to_owned(), forked_metadata.clone());
    compare(
        10,
        "fork",
        fork,
        normalize_session_metadata(&forked_metadata, ""),
        &mut oracle,
    );

    let request = oracle_request(
        11,
        "find_entries",
        json!({"key": "fork", "query": {"order": "oldestFirst"}}),
    );
    let entries = sessions["fork"]
        .find_entries(EntryQuery {
            order: Some(pi_agent_core::harness::session::EntryOrder::OldestFirst),
            ..EntryQuery::default()
        })
        .await
        .unwrap();
    compare(11, "find_entries", request, entries_json(&entries), &mut oracle);

    let open = oracle_request(12, "open", json!({"key": "opened", "sourceKey": "fork"}));
    let opened = repo.open(&forked_metadata).await.unwrap();
    let opened_metadata = opened.get_metadata().await.unwrap();
    sessions.insert("opened".to_owned(), opened);
    compare(
        12,
        "open",
        open,
        normalize_session_metadata(&opened_metadata, ""),
        &mut oracle,
    );

    let request = oracle_request(13, "get_label", json!({"key": "opened", "entryId": "n1"}));
    let label = sessions["opened"].get_label("n1").await.unwrap();
    compare(13, "get_label", request, json!(label), &mut oracle);
}

#[tokio::test]
async fn search_results_match_the_pinned_typescript_source() {
    let mut oracle = TsOracle::spawn(ORACLE, SOURCE_MODULE);
    let repo =
        pi_agent_core::harness::session::InMemorySessionRepo::with_clock(std::sync::Arc::new(|| 1_700_000_000_000));
    expect_ok(&oracle.request(&oracle_request(
        1,
        "reset",
        json!({"root": "/tmp/search", "mode": "memory", "clock": 1_700_000_000_000_i64}),
    )));

    let mut readables: Vec<std::sync::Arc<dyn ScanningReadable<SessionMetadata>>> = Vec::new();
    for (key, id, text) in [
        ("a", "root", "fix auth flow"),
        ("b", "other", "auth in another workspace"),
    ] {
        let create = oracle_request(2, "create", json!({"key": key, "sessionId": id}));
        let session = repo
            .create(SessionCreateOptions {
                id: Some(id.to_owned()),
                parent_session_id: None,
            })
            .await
            .unwrap();
        expect_ok(&oracle.request(&create));
        let entry = json!({"type": "message", "id": format!("{id}-entry"), "message": {"role": "user", "content": [{"type": "text", "text": text}], "timestamp": 1}});
        let append = oracle_request(3, "append_entry", json!({"key": key, "lane": "main", "entry": entry}));
        session.append_entry(fixture_entry(entry), "main").await.unwrap();
        expect_ok(&oracle.request(&append));
        let label = oracle_request(
            4,
            "set_label",
            json!({"key": key, "entryId": format!("{id}-entry"), "label": "important label"}),
        );
        session
            .set_label(&format!("{id}-entry"), Some("important label"))
            .await
            .unwrap();
        expect_ok(&oracle.request(&label));
        readables.push(std::sync::Arc::new(session));
    }

    let rust_search = create_scanning_session_search::<SessionMetadata, (), ScanningSessionSearchHit>(
        ScanningSource::Readables(readables),
        ScanningSessionSearchOptions::default(),
    );
    let cases = [
        json!({"text": "auth", "keys": ["a", "b"]}),
        json!({"text": "important", "keys": ["a", "b"]}),
        json!({"text": "auth", "keys": ["a", "b"], "entryTypes": ["message"]}),
        json!({"text": "auth", "keys": ["a", "b"], "limit": 1}),
        json!({"text": "missing", "keys": ["a", "b"]}),
        json!({"text": "auth", "keys": ["a", "b"], "aborted": true}),
    ];
    for (index, case) in cases.iter().enumerate() {
        let id = 20 + index as u64;
        let request = oracle_request(id, "search", case.clone());
        let response = oracle.request(&request);
        if case.get("aborted").and_then(Value::as_bool) == Some(true) {
            assert_eq!(response["ok"], json!(false));
            assert_eq!(response["error"]["name"], json!("AbortError"));
            let token = tokio_util::sync::CancellationToken::new();
            token.cancel();
            let error = rust_search
                .search(
                    case["text"].as_str().unwrap(),
                    SessionSearchOptions {
                        signal: Some(token),
                        ..SessionSearchOptions::default()
                    },
                )
                .await
                .unwrap_err();
            assert_eq!(error.to_string(), response["error"]["message"].as_str().unwrap());
            continue;
        }
        let output = expect_ok(&response);
        let hits = rust_search
            .search(
                case["text"].as_str().unwrap(),
                SessionSearchOptions {
                    entry_types: case.get("entryTypes").and_then(Value::as_array).map(|types| {
                        types
                            .iter()
                            .map(|value| serde_json::from_value(value.clone()).unwrap())
                            .collect()
                    }),
                    limit: case.get("limit").and_then(Value::as_u64).map(|limit| limit as usize),
                    signal: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(output, &serde_json::to_value(hits).unwrap(), "search case {case}");
    }
}

#[tokio::test]
async fn session_context_matches_the_pinned_typescript_source() {
    let mut oracle = TsOracle::spawn(ORACLE, SOURCE_MODULE);
    let entries = vec![
        json!({"type": "message", "id": "old", "parentId": null, "seq": 1, "timestamp": 1, "message": {"role": "user", "content": [{"type": "text", "text": "old"}], "timestamp": 1}}),
        json!({"type": "compaction", "id": "compact", "parentId": "old", "seq": 2, "timestamp": 2, "summary": "summary", "tokensBefore": 100, "retainedTail": [{"role": "user", "content": [{"type": "text", "text": "retained"}], "timestamp": 1}]}),
        json!({"type": "model_change", "id": "model", "parentId": "compact", "seq": 3, "timestamp": 3, "provider": "openai", "modelId": "gpt-5"}),
        json!({"type": "thinking_level_change", "id": "thinking", "parentId": "model", "seq": 4, "timestamp": 4, "thinkingLevel": "high"}),
        json!({"type": "custom", "id": "custom", "parentId": "thinking", "seq": 5, "timestamp": 5, "customType": "note", "data": "project me"}),
        json!({"type": "message", "id": "tail", "parentId": "custom", "seq": 6, "timestamp": 6, "message": {"role": "user", "content": [{"type": "text", "text": "tail"}], "timestamp": 1}}),
    ];

    for (id, drop_compaction, project_note) in [(1, false, false), (2, true, false), (3, false, true)] {
        let request = oracle_request(
            id,
            "context",
            json!({"entries": entries, "dropCompaction": drop_compaction, "projectNote": project_note}),
        );
        let response = oracle.request(&request);
        let output = expect_ok(&response).clone();

        let parsed: Vec<pi_agent_core::harness::session::Entry> = entries
            .iter()
            .map(|entry| serde_json::from_value(entry.clone()).unwrap())
            .collect();
        let mut options = SessionContextBuildOptions::default();
        if drop_compaction {
            options.entry_transforms.push(std::sync::Arc::new(
                |entries: &[pi_agent_core::harness::session::Entry]| {
                    entries
                        .iter()
                        .filter(|entry| entry.entry_type() != pi_agent_core::harness::session::EntryType::Compaction)
                        .cloned()
                        .collect()
                },
            ));
        }
        if project_note {
            options.entry_projectors.insert(
                "note".to_owned(),
                std::sync::Arc::new(|entry: &pi_agent_core::harness::session::Entry, _index, _entries| {
                    let pi_agent_core::harness::session::EntryBody::Custom { data, .. } = &entry.body else {
                        return None;
                    };
                    Some(vec![pi_agent_core::agent::AgentMessage::Custom(
                        json!({"role": "user", "content": [{"type": "text", "text": format!("note: {}", data.as_value().and_then(Value::as_str).unwrap_or_default())}], "timestamp": entry.timestamp})
                            .as_object()
                            .unwrap()
                            .clone(),
                    )])
                }),
            );
        }
        let context = build_session_context(&parsed, &options);
        let rust = json!({
            "messages": context.messages,
            "thinkingLevel": context.thinking_level,
            "model": context.model,
            "activeToolNames": context.active_tool_names,
        });
        assert_eq!(output, rust, "context case {id}");
    }
}

#[tokio::test]
async fn jsonl_search_matches_the_pinned_typescript_source() {
    let ts_root = TempDir::new().unwrap();
    let rust_root = TempDir::new().unwrap();
    let mut oracle = TsOracle::spawn(ORACLE, SOURCE_MODULE);
    let repo = jsonl_repo(rust_root.path());
    expect_ok(&oracle.request(&oracle_request(
        1,
        "reset",
        json!({"root": ts_root.path(), "mode": "jsonl", "clock": 1_700_000_000_000_i64}),
    )));

    for (key, id, cwd, text) in [
        ("a", "jsonl", "workspace", "jsonl backed auth entry"),
        ("b", "other", "other", "jsonl backed auth entry in another cwd"),
    ] {
        let create = oracle_request(2, "create", json!({"key": key, "sessionId": id, "cwd": cwd}));
        let session = repo
            .create(JsonlSessionCreateOptions {
                id: Some(id.to_owned()),
                parent_session_id: None,
                cwd: rust_root.path().join(cwd).to_string_lossy().into_owned(),
                metadata: None,
            })
            .await
            .unwrap();
        expect_ok(&oracle.request(&create));
        let entry = json!({"type": "message", "id": format!("{id}-entry"), "message": {"role": "user", "content": [{"type": "text", "text": text}], "timestamp": 1}});
        let append = oracle_request(3, "append_entry", json!({"key": key, "lane": "main", "entry": entry}));
        session.append_entry(fixture_entry(entry), "main").await.unwrap();
        expect_ok(&oracle.request(&append));
        let label = oracle_request(
            4,
            "set_label",
            json!({"key": key, "entryId": format!("{id}-entry"), "label": "disk label"}),
        );
        session
            .set_label(&format!("{id}-entry"), Some("disk label"))
            .await
            .unwrap();
        expect_ok(&oracle.request(&label));
    }

    let options = JsonlSessionRepoOptions {
        fs: std::sync::Arc::new(NativeFileSystem::new(rust_root.path())),
        sessions_root: rust_root.path().to_string_lossy().into_owned(),
    };
    let source = std::sync::Arc::new(move |query: Option<JsonlSessionListOptions>| {
        let options = options.clone();
        let query = query.unwrap_or_default();
        Box::pin(async move {
            let mut readables: Vec<std::sync::Arc<dyn ScanningReadable<JsonlSessionMetadata>>> = Vec::new();
            for metadata in pi_agent_core::harness::session::list_jsonl_session_metadata(&options, &query).await? {
                let storage = pi_agent_core::harness::session::load_jsonl_session_storage(&options, &metadata).await?;
                readables.push(std::sync::Arc::new(Session::new(std::sync::Arc::new(storage))));
            }
            Ok(readables)
        }) as pi_ai::request::BoxFuture<'static, _>
    });
    let search = create_scanning_session_search::<
        JsonlSessionMetadata,
        JsonlSessionListOptions,
        ScanningSessionSearchHit,
    >(ScanningSource::Dynamic(source), ScanningSessionSearchOptions::default());

    for (index, text) in ["auth", "disk", "missing"].iter().enumerate() {
        let id = 10 + index as u64;
        let request = oracle_request(id, "search", json!({"source": "jsonl", "text": text}));
        let response = oracle.request(&request);
        let output = expect_ok(&response);
        let hits = search.search(text, SessionSearchOptions::default()).await.unwrap();
        assert_eq!(output, &serde_json::to_value(hits).unwrap(), "jsonl search {text}");
    }

    let request = oracle_request(
        20,
        "search",
        json!({"source": "jsonl", "text": "auth", "entryTypes": ["custom"]}),
    );
    let response = oracle.request(&request);
    let hits = search
        .search(
            "auth",
            SessionSearchOptions {
                entry_types: Some(vec![pi_agent_core::harness::session::EntryType::Custom]),
                ..SessionSearchOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(expect_ok(&response), &serde_json::to_value(hits).unwrap());

    let request = oracle_request(21, "files", json!({}));
    assert_eq!(
        expect_ok(&oracle.request(&request)),
        &Value::Object(read_files(rust_root.path()))
    );

    let _ = (EntryQuery::default(), BranchQuery::default(), RecordQuery::default());
    let _ = (fixture_record, record_json, records_json, entry_json);
}
