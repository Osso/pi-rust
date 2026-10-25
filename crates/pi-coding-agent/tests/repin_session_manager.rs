use super::repin_storage_history as oracle;

use pi_agent_core::agent::AgentMessage;
use pi_coding_agent::core::session_manager::{SessionManager, SessionManagerError, SetMetadataControlDbPathOptions};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use std::fs;

fn source() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "session-manager-repin-oracle.mts",
        "packages/coding-agent/src/core/session-manager.ts",
    )
}

fn call(ts: &mut oracle::TsOracle, request: Value) -> Value {
    let response = ts.request(&request);
    assert_eq!(response["ok"], true, "{response}");
    response["output"].clone()
}

fn native_error(error: SessionManagerError) -> Value {
    match error.native_error() {
        Some(native) => native.value().to_serde().unwrap(),
        None => json!({"name":error.name(), "message":error.message()}),
    }
}

fn user(text: &str) -> AgentMessage {
    serde_json::from_value(json!({"role":"user","content":text,"timestamp":1})).unwrap()
}

fn created(ts: &mut oracle::TsOracle, dir: &str, id: &str) -> String {
    let out = call(
        ts,
        json!({"id":1,"mode":"create","cwd":dir,"dir":dir,"sessionId":id,
        "commands":[{"op":"user","text":"original"},{"op":"persist"}]}),
    );
    let header = &out["state"]["header"];
    let timestamp = header["timestamp"].as_str().unwrap().replace([':', '.'], "-");
    format!("{dir}/{timestamp}_{id}.jsonl")
}

#[test]
fn append_recreates_removed_flushed_transcript_like_frozen() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    let path = created(&mut ts, dir, "removed");
    let mut manager = SessionManager::open(&path.as_str().into(), None, None).unwrap();
    let peer = call(
        &mut ts,
        json!({"id":2,"mode":"open","path":path,
        "commands":[{"op":"remove"},{"op":"user","text":"after removal"}]}),
    );
    fs::remove_file(&path).unwrap();
    manager.append_message(user("after removal")).unwrap();
    let records: Vec<Value> = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let expected: Vec<Value> = peer["state"]["bytes"]
        .as_str()
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        records.len(),
        1,
        "appendFileSync creates a new file, not a replacement header"
    );
    assert_eq!(records[0]["message"], expected[0]["message"]);
    assert_eq!(records[0]["parentId"], expected[0]["parentId"]);
    assert_eq!(manager.get_entries().len(), 2);
}

#[test]
fn empty_control_db_skips_sandbox_persistence_like_frozen() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    let expected = call(
        &mut ts,
        json!({"id":1,"mode":"create","cwd":dir,"dir":dir,"sessionId":"empty-db",
        "commands":[{"op":"db","path":""},{"op":"sandbox"}]}),
    );
    let mut manager = SessionManager::create(&dir.into(), Some(&dir.into()), None).unwrap();
    manager
        .set_metadata_control_db_path(Some(&JsString::default()), SetMetadataControlDbPathOptions::default())
        .unwrap();
    let actual = manager.set_session_sandbox_profile(None).unwrap();
    assert_eq!(json!(actual), expected["results"][1]);
    assert!(!std::path::Path::new(manager.get_session_file().unwrap().to_utf8().unwrap().as_str()).exists());
}

#[test]
fn in_memory_file_switch_rejects_archives_like_frozen() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let path = format!("{dir}/missing.jsonl.zst");
    let mut ts = source();
    let peer = ts.request(&json!({"id":1,"mode":"memory","cwd":dir,"commands":[{"op":"switch","path":path}]}));
    assert_eq!(peer["ok"], false);
    let mut manager = SessionManager::in_memory(Some(&dir.into()), None).unwrap();
    let actual = manager.set_session_file(&path.as_str().into()).unwrap_err();
    assert_eq!(native_error(actual), peer["error"]);
    assert_eq!(manager.get_session_file(), Some(&JsString::from(path)));
}

#[test]
fn filesystem_failures_keep_frozen_error_records_and_append_state() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    let path = created(&mut ts, dir, "errors");
    let mut manager = SessionManager::open(&path.as_str().into(), None, None).unwrap();
    let peer = ts.request(&json!({"id":2,"mode":"open","path":path,
        "commands":[{"op":"directory"},{"op":"user","text":"failed append"}]}));
    assert_eq!(peer["ok"], false);
    let actual = manager.append_message(user("failed append")).unwrap_err();
    assert_eq!(native_error(actual), peer["error"]);
    assert_eq!(
        manager.get_entries().len(),
        peer["state"]["entries"].as_array().unwrap().len()
    );
    assert_eq!(
        manager.get_leaf_entry().unwrap().message().unwrap().as_ref(),
        &user("failed append")
    );

    let parent_file = format!("{dir}/not-a-directory");
    fs::write(&parent_file, "file").unwrap();
    let blocked_dir = format!("{parent_file}/sessions");
    let peer = ts.request(&json!({"id":3,"mode":"create","cwd":dir,"dir":blocked_dir}));
    let actual = SessionManager::create(&dir.into(), Some(&blocked_dir.into()), None)
        .err()
        .unwrap();
    assert_eq!(native_error(actual), peer["error"]);

    let peer = ts.request(&json!({"id":4,"mode":"load","path":dir}));
    let actual = pi_coding_agent::core::session_manager::load_entries_from_file(&dir.into()).unwrap_err();
    assert_eq!(native_error(actual), peer["error"]);
}

#[test]
fn initial_persistence_and_fork_failures_keep_frozen_fs_boundary() {
    use pi_coding_agent::core::session_manager::NewSessionOptions;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    let missing = format!("{dir}/explicit.jsonl");
    let mut manager = SessionManager::open(&missing.as_str().into(), None, None).unwrap();
    let peer = ts.request(&json!({"id":1,"mode":"open","path":missing,
        "commands":[{"op":"directory"},{"op":"persist"}]}));
    assert_eq!(peer["ok"], false);
    assert_eq!(native_error(manager.persist_for_recovery().unwrap_err()), peer["error"]);
    fs::remove_dir(&missing).unwrap();

    let path = created(&mut ts, dir, "fork-errors");
    let blocked = format!("{dir}/blocked");
    fs::write(&blocked, "not a directory").unwrap();
    let peer = ts.request(&json!({"id":2,"mode":"fork","path":path,"cwd":dir,"dir":blocked,"sessionId":"forked"}));
    assert_eq!(peer["ok"], false);
    let actual = SessionManager::fork_from(
        &path.as_str().into(),
        &dir.into(),
        Some(&blocked.into()),
        Some(NewSessionOptions {
            id: Some("forked".into()),
            ..Default::default()
        }),
        None,
    )
    .err()
    .unwrap();
    let mut actual = native_error(actual);
    let expected = &peer["error"];
    // The destination file includes a fresh clock; only that generated filename differs.
    if let (Some(a), Some(b)) = (actual["path"].as_str(), expected["path"].as_str()) {
        let message = actual["message"].as_str().unwrap().replace(a, b);
        actual["message"] = json!(message);
        actual["path"] = expected["path"].clone();
    }
    assert_eq!(actual, *expected);
}

#[test]
fn recent_session_uses_frozen_millisecond_ties_and_filename_order() {
    use pi_coding_agent::core::session_manager::find_most_recent_session;
    use std::time::{Duration, UNIX_EPOCH};
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    // Create b first so filesystem enumeration is not already alphabetic.
    for (name, nanos) in [("b", 400_000), ("a", 100_000)] {
        let path = created(&mut ts, dir, name);
        let renamed = format!("{dir}/{name}.jsonl");
        fs::rename(path, &renamed).unwrap();
        let file = fs::File::options().write(true).open(&renamed).unwrap();
        let modified = UNIX_EPOCH + Duration::new(1_700_000_000, nanos);
        file.set_times(fs::FileTimes::new().set_modified(modified)).unwrap();
    }
    let expected = call(&mut ts, json!({"id":1,"mode":"recent","dir":dir,"cwd":dir}));
    assert_eq!(expected, json!(format!("{dir}/a.jsonl")));
    assert_eq!(
        json!(find_most_recent_session(&dir.into(), Some(&dir.into())).unwrap()),
        expected
    );
    // Node Stats rounds, rather than truncates, before creating Date objects.
    for (name, nanos) in [("b", 1_100_000), ("a", 900_000)] {
        fs::File::options()
            .write(true)
            .open(format!("{dir}/{name}.jsonl"))
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::new(1_700_000_000, nanos)))
            .unwrap();
    }
    let expected = call(&mut ts, json!({"id":2,"mode":"recent","dir":dir,"cwd":dir}));
    assert_eq!(expected, json!(format!("{dir}/a.jsonl")));
    assert_eq!(
        json!(find_most_recent_session(&dir.into(), Some(&dir.into())).unwrap()),
        expected
    );
}

#[test]
fn cross_session_goal_lookup_resolves_existing_archive_before_database_read() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    let path = created(&mut ts, dir, "archived-goal");
    let db = format!("{dir}/control.sqlite");
    let mut manager = SessionManager::open(&path.as_str().into(), None, None).unwrap();
    manager
        .set_metadata_control_db_path(Some(&db.as_str().into()), SetMetadataControlDbPathOptions::default())
        .unwrap();
    let expected = call(
        &mut ts,
        json!({"id":2,"mode":"open","path":path,"commands":[
        {"op":"db","path":db},{"op":"archiveGoal","text":"{\"task\":\"restore\"}"}]}),
    );
    assert_eq!(expected["results"][1], json!("{\"task\":\"restore\"}"));
    assert_eq!(
        json!(
            manager
                .get_session_goal_json_for_session(&path.as_str().into())
                .unwrap()
        ),
        expected["results"][1]
    );
}

#[test]
fn relocation_keeps_source_file_mode_and_frozen_copy_errors() {
    use pi_coding_agent::core::session_manager::get_default_session_dir;
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_str().unwrap();
    let agent: JsString = format!("{root}/agent").into();
    let old_cwd: JsString = format!("{root}/old").into();
    let new_cwd: JsString = format!("{root}/new").into();
    let dir = get_default_session_dir(&old_cwd, Some(&agent))
        .unwrap()
        .to_utf8()
        .unwrap();
    let mut ts = source();
    let path = created(&mut ts, &dir, "relocate-mode");
    let bytes = fs::read(&path).unwrap();
    // Use the actual project cwd so both managers recognize the default directory.
    let mut header_lines: Vec<Value> = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    header_lines[0]["cwd"] = json!(old_cwd);
    let bytes = header_lines
        .iter()
        .map(|entry| entry.to_string() + "\n")
        .collect::<String>();
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let mut manager = SessionManager::open(&path.as_str().into(), None, None).unwrap();
    let expected = call(
        &mut ts,
        json!({"id":1,"mode":"open","path":path,"commands":[{"op":"relocate","text":new_cwd,"path":agent}]}),
    );
    let target = expected["state"]["path"].as_str().unwrap();
    let expected_mode = fs::metadata(target).unwrap().permissions().mode() & 0o777;
    assert_eq!(expected_mode, 0o600);
    fs::remove_file(target).unwrap();
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    manager.relocate(&new_cwd, Some(&agent)).unwrap();
    assert_eq!(
        fs::metadata(target).unwrap().permissions().mode() & 0o777,
        expected_mode
    );
    assert_eq!(fs::read_to_string(target).unwrap(), bytes);
    assert!(!std::path::Path::new(&path).exists());
}

#[test]
fn relocation_copy_failure_keeps_original_two_path_error() {
    use pi_coding_agent::core::session_manager::get_default_session_dir;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_str().unwrap();
    let agent: JsString = format!("{root}/agent").into();
    let old_cwd: JsString = format!("{root}/old").into();
    let new_cwd: JsString = format!("{root}/new").into();
    let dir = get_default_session_dir(&old_cwd, Some(&agent)).unwrap();
    let mut ts = source();
    let created = call(
        &mut ts,
        json!({"id":1,"mode":"create","cwd":old_cwd,"dir":dir,"sessionId":"copy-errors","commands":[{"op":"persist"}]}),
    );
    let path = created["state"]["path"].as_str().unwrap();
    let mut manager = SessionManager::open(&path.into(), None, None).unwrap();
    let peer = ts.request(&json!({"id":2,"mode":"open","path":path,"commands":[{"op":"directory"},{"op":"relocate","text":new_cwd,"path":agent}]}));
    assert_eq!(peer["ok"], false);
    let actual = manager.relocate(&new_cwd, Some(&agent)).unwrap_err();
    assert_eq!(native_error(actual), peer["error"]);
}

#[test]
fn reciprocal_legacy_file_migration_rewrites_exact_frozen_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    for version in [1, 2] {
        let created = call(
            &mut ts,
            json!({"id":1,"mode":"create","cwd":dir,"dir":dir,"sessionId":format!("legacy{version}"),"commands":[
            {"op":"user","text":"legacy user 😀"},{"op":"thinking","text":"high"},
            {"op":"custom","text":"legacy state"},{"op":"legacy","text":version.to_string()}]}),
        );
        let path = created["state"]["path"].as_str().unwrap();
        let original = fs::read(path).unwrap();
        let imported = String::from_utf8(original.clone())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect::<Vec<Value>>();
        let expected = call(&mut ts, json!({"id":2,"mode":"open","path":path}));
        fs::write(path, original).unwrap();
        let manager = SessionManager::open(&path.into(), None, None).unwrap();
        assert_reciprocal_state(snapshot(&manager), expected["state"].clone(), &imported);
        let read = call(&mut ts, json!({"id":3,"mode":"open","path":path}));
        assert_eq!(snapshot(&manager), read["state"]);
        assert_eq!(manager.get_header().unwrap().version(), Some(3.0));
    }
}

fn info_value(info: pi_coding_agent::core::session_manager::SessionInfo) -> Value {
    let mut value = json!({"path":info.path,"id":info.id,"cwd":info.cwd,
        "created":pi_js_compat::date::to_iso_string(info.created as i64),
        "modified":pi_js_compat::date::to_iso_string(info.modified as i64),
        "messageCount":info.message_count,"firstMessage":info.first_message});
    if let Some(parent) = info.parent_session_path {
        value["parentSessionPath"] = json!(parent);
    }
    if let Some(name) = info.name {
        value["name"] = json!(name);
    }
    if let Some(archived) = info.is_archived {
        value["isArchived"] = json!(archived);
    }
    value
}

#[test]
fn reciprocal_session_info_listing_progress_and_continue_use_persisted_records() {
    use pi_coding_agent::core::session_manager::build_session_info;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    let first = created(&mut ts, dir, "listed-first");
    let mut native = SessionManager::create(
        &dir.into(),
        Some(&dir.into()),
        Some(pi_coding_agent::core::session_manager::NewSessionOptions {
            id: Some("listed-native".into()),
            ..Default::default()
        }),
    )
    .unwrap();
    native.append_message(user("native prompt 😀")).unwrap();
    native.append_message(assistant("native response")).unwrap();
    fs::write(format!("{dir}/invalid.jsonl"), "not JSON\n").unwrap();
    for path in [&first, &native.get_session_file().unwrap().to_utf8().unwrap()] {
        let expected = call(&mut ts, json!({"id":1,"mode":"info","path":path}));
        assert_eq!(info_value(build_session_info(&path.as_str().into()).unwrap()), expected);
    }
    for all in [false, true] {
        let expected = call(
            &mut ts,
            json!({"id":2,"mode":if all {"listAll"} else {"list"},"cwd":dir,"dir":dir}),
        );
        let mut progress = Vec::new();
        let mut on_progress = |loaded, total| progress.push(json!([loaded, total]));
        let actual = if all {
            SessionManager::list_all(Some(&dir.into()), Some(&mut on_progress), None)
        } else {
            SessionManager::list(&dir.into(), Some(&dir.into()), Some(&mut on_progress), None)
        }
        .unwrap();
        assert_eq!(
            json!({"sessions":actual.into_iter().map(info_value).collect::<Vec<_>>(),"progress":progress}),
            expected
        );
    }
    let expected = call(&mut ts, json!({"id":3,"mode":"continue","cwd":dir,"dir":dir}));
    let resumed = SessionManager::continue_recent(&dir.into(), Some(&dir.into())).unwrap();
    assert_eq!(snapshot(&resumed), expected["state"]);
}

#[test]
fn path_conversion_preserves_frozen_native_error_identity() {
    let mut ts = source();
    for path in ["file:///tmp/encoded%2Fseparator", "file://remote/tmp/session"] {
        let expected = ts.request(&json!({"id":1,"mode":"create","cwd":"/tmp","dir":path}));
        assert_eq!(expected["ok"], false);
        let actual = SessionManager::create(&"/tmp".into(), Some(&path.into()), None)
            .err()
            .unwrap();
        assert_eq!(native_error(actual), expected["error"]);
        let expected = ts.request(&json!({"id":2,"mode":"recent","cwd":"/tmp","dir":path}));
        let actual = pi_coding_agent::core::session_manager::find_most_recent_session(&path.into(), None).unwrap_err();
        assert_eq!(native_error(actual), expected["error"]);
    }
}

#[test]
fn default_directory_helper_skips_existing_paths_like_frozen() {
    use pi_coding_agent::core::session_manager::{get_default_session_dir, get_default_session_dir_path};
    let tmp = tempfile::tempdir().unwrap();
    let agent = tmp.path().to_str().unwrap();
    let path = get_default_session_dir_path(&"/project".into(), &agent.into()).unwrap();
    let path_text = path.to_utf8().unwrap();
    fs::create_dir_all(std::path::Path::new(&path_text).parent().unwrap()).unwrap();
    fs::write(&path_text, "existing file").unwrap();
    let mut ts = source();
    let expected = call(&mut ts, json!({"id":1,"mode":"default","cwd":"/project","dir":agent}));
    assert_eq!(
        json!(get_default_session_dir(&"/project".into(), Some(&agent.into())).unwrap()),
        expected
    );
    assert_eq!(fs::read_to_string(&path_text).unwrap(), "existing file");
}

fn assistant(text: &str) -> AgentMessage {
    serde_json::from_value(json!({"role":"assistant","content":[{"type":"text","text":text}],
        "api":"anthropic-messages","provider":"anthropic","model":"fixture",
        "usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},
        "stopReason":"stop","timestamp":2}))
    .unwrap()
}

fn snapshot(manager: &SessionManager) -> Value {
    let path = manager.get_session_file().map(|path| path.to_utf8().unwrap());
    let context = manager.build_session_context();
    let context = json!({"messages": context.messages, "thinkingLevel":context.thinking_level,
        "model":context.model.map(|model| json!({"provider":model.provider,"modelId":model.model_id}))});
    json!({"path":path, "header":manager.get_header(), "entries":manager.get_entries(),
        "tree":manager.get_tree(), "context":context,
        "contextEntries":manager.build_context_entries(), "cwd":manager.get_cwd(),
        "leaf":manager.get_leaf_id(), "name":manager.get_session_name(),
        "nameState":manager.has_session_name_state(), "persisted":manager.is_persisted(),
        "bytes":path.and_then(|path| fs::read_to_string(path).ok())})
}

// Only clocks/IDs allocated by each implementation differ. Imported bytes, all
// field order, references, values and omitted fields must remain exact.
fn assert_reciprocal_state(mut actual: Value, mut expected: Value, imported: &[Value]) {
    let imported_ids: Vec<&Value> = imported.iter().map(|record| &record["id"]).collect();
    let actual_records: Vec<Value> = std::iter::once(actual["header"].clone())
        .chain(actual["entries"].as_array().unwrap().iter().cloned())
        .collect();
    let expected_records: Vec<Value> = std::iter::once(expected["header"].clone())
        .chain(expected["entries"].as_array().unwrap().iter().cloned())
        .collect();
    assert_eq!(actual_records.len(), expected_records.len());
    let mut replacements = Vec::new();
    for (native, source) in actual_records.iter().zip(&expected_records) {
        if imported_ids.contains(&&source["id"]) {
            continue;
        }
        for record in [native, source] {
            let id = record["id"].as_str().unwrap();
            assert!(id.len() == 8 || (id.len() == 36 && &id[14..15] == "7"), "{id}");
            assert!(pi_js_compat::date::date_parse(record["timestamp"].as_str().unwrap()).is_finite());
        }
        replacements.push((source["id"].clone(), native["id"].clone()));
    }
    fn normalize_record(value: &mut Value, imported: &[&Value], clocks: &[Value]) {
        match value {
            Value::Object(object) => {
                let generated = object.get("id").is_some_and(|id| !imported.contains(&id));
                if generated && object.contains_key("timestamp") {
                    object.insert("timestamp".into(), json!("<generated-clock>"));
                }
                if object.get("labelTimestamp").is_some_and(|value| clocks.contains(value)) {
                    object.insert("labelTimestamp".into(), json!("<generated-clock>"));
                }
                let projected_summary = object
                    .get("role")
                    .is_some_and(|role| role == "compactionSummary" || role == "branchSummary" || role == "custom");
                if projected_summary && object.get("timestamp").is_some_and(|value| clocks.contains(value)) {
                    object.insert("timestamp".into(), json!("<generated-clock>"));
                }
                for value in object.values_mut() {
                    normalize_record(value, imported, clocks);
                }
            }
            Value::Array(array) => {
                for value in array {
                    normalize_record(value, imported, clocks);
                }
            }
            _ => {}
        }
    }
    let native_path = actual["path"].as_str().unwrap().to_owned();
    let source_path = expected["path"].as_str().unwrap().to_owned();
    // Paths generated from timestamp/id must use the corresponding header fields.
    for (state, path) in [(&actual, &native_path), (&expected, &source_path)] {
        if !imported_ids.contains(&&state["header"]["id"]) {
            let stamp = state["header"]["timestamp"].as_str().unwrap().replace([':', '.'], "-");
            let suffix = format!("{stamp}_{}.jsonl", state["header"]["id"].as_str().unwrap());
            assert!(path.ends_with(&suffix), "{path}");
        }
    }
    replacements.push((json!(source_path), json!(native_path)));
    fn replace(value: &mut Value, pairs: &[(Value, Value)]) {
        if let Some((_, target)) = pairs.iter().find(|(source, _)| value == source) {
            *value = target.clone();
            return;
        }
        match value {
            Value::Object(object) => {
                for value in object.values_mut() {
                    replace(value, pairs);
                }
            }
            Value::Array(array) => {
                for value in array {
                    replace(value, pairs);
                }
            }
            _ => {}
        }
    }
    // Compare actual wire strings, substituting only generated fields in place;
    // no sort, re-serialization, or broad output canonicalization is used.
    let mut source_bytes = expected["bytes"].as_str().unwrap().to_owned();
    let mut native_bytes = actual["bytes"].as_str().unwrap().to_owned();
    for (native, source) in actual_records.iter().zip(&expected_records) {
        if imported_ids.contains(&&source["id"]) {
            continue;
        }
        for (bytes, record) in [(&mut source_bytes, source), (&mut native_bytes, native)] {
            let token = format!("\"timestamp\":{}", record["timestamp"]);
            *bytes = bytes.replace(&token, "\"timestamp\":\"<generated-clock>\"");
        }
    }
    for (from, to) in &replacements {
        source_bytes = source_bytes.replace(&from.to_string(), &to.to_string());
    }
    assert_eq!(native_bytes, source_bytes, "reciprocal JSONL byte contract");
    actual.as_object_mut().unwrap().remove("bytes");
    expected.as_object_mut().unwrap().remove("bytes");
    for (state, records) in [(&mut actual, &actual_records), (&mut expected, &expected_records)] {
        let clocks = records
            .iter()
            .filter(|record| !imported_ids.contains(&&record["id"]))
            .flat_map(|record| {
                [
                    record["timestamp"].clone(),
                    json!(pi_js_compat::date::date_parse(record["timestamp"].as_str().unwrap()) as i64),
                ]
            })
            .collect::<Vec<_>>();
        normalize_record(state, &imported_ids, &clocks);
    }
    replace(&mut expected, &replacements);
    assert_values(&actual, &expected, "state");
}

fn assert_values(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: keys {:?} vs {:?}", a.keys(), b.keys());
            for (key, value) in a {
                assert_values(value, &b[key], &format!("{path}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_values(a, b, &format!("{path}[{i}]"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
fn reciprocal_lifecycle_preserves_all_entry_bytes_tree_labels_and_context() {
    use pi_coding_agent::core::session_manager::{CompactionAppendOptions, NewSessionOptions};
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_str().unwrap();
    let mut ts = source();
    let path = created(&mut ts, dir, "lifecycle");
    let initial = fs::read(&path).unwrap();
    let imported: Vec<Value> = String::from_utf8(initial.clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let root = imported[1]["id"].as_str().unwrap();
    let commands = json!([
        {"op":"thinking","text":"high"}, {"op":"model","text":"fixture"},
        {"op":"assistant","text":"First response 😀"},
        {"op":"label","target":root,"text":"bookmark"},
        {"op":"custom","text":"state 漢字"},
        {"op":"branch","target":root},
        {"op":"summary","target":root,"text":"abandoned branch"},
        {"op":"user","text":"retained user"},
        {"op":"compaction","target":"$7","text":"summary"},
        {"op":"update","target":"$8","text":"  edited summary  "},
        {"op":"label","target":"$7","text":"keep"},
        {"op":"label","target":root},
        {"op":"assistant","text":"After compaction"}
    ]);
    let expected = call(&mut ts, json!({"id":2,"mode":"open","path":path,"commands":commands}));
    fs::write(&path, &initial).unwrap();
    let mut manager = SessionManager::open(&path.as_str().into(), None, None).unwrap();
    manager.append_thinking_level_change(&"high".into()).unwrap();
    manager
        .append_model_change(&"anthropic".into(), &"fixture".into())
        .unwrap();
    manager.append_message(assistant("First response 😀")).unwrap();
    manager
        .append_label_change(&root.into(), Some(&"bookmark".into()))
        .unwrap();
    manager
        .append_custom_entry(
            &"state".into(),
            Some(json!({"text":"state 漢字","nested":{"n":2}}).into()),
        )
        .unwrap();
    manager.branch(&root.into()).unwrap();
    manager
        .branch_with_summary(
            Some(&root.into()),
            &"abandoned branch".into(),
            Some(json!({"source":"repin"}).into()),
            Some(false),
        )
        .unwrap();
    let kept = manager.append_message(user("retained user")).unwrap();
    let compaction = manager
        .append_compaction(
            &"summary".into(),
            &kept,
            123.0,
            CompactionAppendOptions {
                details: Some(json!({"source":"repin"}).into()),
                from_hook: Some(false),
                duration_ms: Some(42.0),
                ..Default::default()
            },
        )
        .unwrap();
    manager
        .update_compaction_summary(&compaction, &"  edited summary  ".into())
        .unwrap();
    manager.append_label_change(&kept, Some(&"keep".into())).unwrap();
    manager.append_label_change(&root.into(), None).unwrap();
    manager.append_message(assistant("After compaction")).unwrap();
    assert_reciprocal_state(snapshot(&manager), expected["state"].clone(), &imported);
    // Both implementations read each other's physical bytes, not an in-memory projection.
    let native_bytes = fs::read(&path).unwrap();
    let read_back = call(&mut ts, json!({"id":3,"mode":"open","path":path}));
    let reopened = SessionManager::open(&path.as_str().into(), None, None).unwrap();
    assert_eq!(snapshot(&reopened), read_back["state"]);
    assert_eq!(fs::read(&path).unwrap(), native_bytes);

    let extracted = call(
        &mut ts,
        json!({"id":6,"mode":"open","path":path,"commands":[{"op":"extract","target":reopened.get_leaf_id()}]}),
    );
    let mut extracted_native = SessionManager::open(&path.as_str().into(), None, None).unwrap();
    extracted_native
        .create_branched_session(reopened.get_leaf_id().unwrap())
        .unwrap();
    let extracted_imports = reopened
        .get_entries()
        .iter()
        .map(|entry| serde_json::to_value(entry).unwrap())
        .collect::<Vec<_>>();
    assert_reciprocal_state(
        snapshot(&extracted_native),
        extracted["state"].clone(),
        &extracted_imports,
    );
    let read = call(
        &mut ts,
        json!({"id":7,"mode":"open","path":extracted_native.get_session_file()}),
    );
    assert_eq!(
        snapshot(&SessionManager::open(extracted_native.get_session_file().unwrap(), None, None).unwrap()),
        read["state"]
    );

    // Full, empty, and selected forks are read by the other implementation.
    for (selection, source_selection) in [
        (None, Value::Null),
        (
            Some(pi_coding_agent::core::session_manager::ForkEntrySelection::None),
            Value::Null,
        ),
        (
            Some(pi_coding_agent::core::session_manager::ForkEntrySelection::From(
                kept.clone(),
            )),
            json!(kept),
        ),
    ] {
        let options = Some(NewSessionOptions {
            id: Some("forked".into()),
            ..Default::default()
        });
        let native = SessionManager::fork_from(
            &path.as_str().into(),
            &dir.into(),
            Some(&dir.into()),
            options,
            selection.clone(),
        )
        .unwrap();
        let native_state = snapshot(&native);
        let native_file = native.get_session_file().unwrap().to_utf8().unwrap();
        let native_bytes = fs::read(&native_file).unwrap();
        fs::remove_file(&native_file).unwrap();
        let mut request = json!({"id":4,"mode":"fork","path":path,"cwd":dir,"dir":dir,"sessionId":"forked"});
        if selection.is_some() {
            request["from"] = source_selection;
        }
        let peer = call(&mut ts, request);
        let mut imported = reopened
            .get_entries()
            .iter()
            .map(|entry| serde_json::to_value(entry).unwrap())
            .collect::<Vec<_>>();
        // A fixed supplied session id still has an independently allocated clock.
        imported.push(json!({"id":"forked"}));
        let mut actual = native_state;
        let mut expected = peer["state"].clone();
        actual["header"]["timestamp"] = expected["header"]["timestamp"].clone();
        let native_stamp = native.get_header().unwrap().timestamp().to_utf8().unwrap();
        let source_stamp = expected["header"]["timestamp"].as_str().unwrap();
        actual["bytes"] = json!(actual["bytes"].as_str().unwrap().replace(&native_stamp, source_stamp));
        actual["path"] = expected["path"].clone();
        assert_reciprocal_state(actual, expected.take(), &imported);
        fs::write(&native_file, &native_bytes).unwrap();
        let read = call(&mut ts, json!({"id":5,"mode":"open","path":native.get_session_file()}));
        assert_eq!(
            snapshot(&SessionManager::open(native.get_session_file().unwrap(), None, None).unwrap()),
            read["state"]
        );
        fs::remove_file(&native_file).unwrap();
        let peer_file = peer["state"]["path"].as_str().unwrap();
        if peer_file != native_file {
            fs::remove_file(peer_file).unwrap();
        }
    }
}
