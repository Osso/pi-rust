#[path = "support/env_probe.rs"]
mod env_probe;
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::{
    core::{
        session_control_db::{WritableSessionMetadata, read_session_metadata, write_session_metadata},
        session_manager::SessionInfo,
    },
    modes::interactive::{
        components::session_selector::{SessionListLoader, SessionSelectorComponent},
        theme::init_theme,
    },
};
use pi_js_compat::js_json::JsString;
use pi_tui::Component;
use serde_json::{Value, json};
use std::{cell::RefCell, path::Path, rc::Rc};

#[test]
fn selector_mutations_match_real_files_and_control_database() {
    use std::os::unix::fs::PermissionsExt;
    let bin = tempfile::tempdir().unwrap();
    let trash = bin.path().join("trash");
    std::fs::write(&trash,"#!/usr/bin/python3\nimport os,sys\nfor path in sys.argv[2:]:\n if os.path.isfile(path): os.unlink(path)\nif any(os.path.isdir(path) for path in sys.argv[2:]):\n print('blocked directory',file=sys.stderr)\n sys.exit(1)\n").unwrap();
    std::fs::set_permissions(&trash, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.path().display(), std::env::var("PATH").unwrap());
    env_probe::run_probe(
        "mutation_probe",
        &[("PATH", path.into()), ("COLORTERM", "truecolor".into())],
    );
}
fn setup(root: &Path, partial: bool, archived: bool) -> Value {
    std::fs::create_dir_all(root).unwrap();
    let db = root.join("control.sqlite");
    let parent = root.join("root.jsonl");
    let child = root.join("child.jsonl");
    std::fs::write(&parent,"{\"type\":\"session\",\"version\":3,\"id\":\"root\",\"cwd\":\"/fixture\",\"timestamp\":\"2026-10-22T00:00:00.000Z\"}\n").unwrap();
    if partial {
        std::fs::create_dir(&child).unwrap();
    } else {
        std::fs::write(&child, "fixture\n").unwrap();
    }
    for (index, path) in [&parent, &child].iter().enumerate() {
        let row:WritableSessionMetadata=serde_json::from_value(json!({"sessionPath":path,"parentSessionPath":if index==0 {Value::Null}else{json!(parent)},"id":index.to_string(),"isSubagent":index>0,"cwd":"/fixture","createdAt":"2200-01-01T00:00:00.000Z","modifiedAt":"2200-01-01T00:00:00.000Z","messageCount":2,"firstMessage":"fixture","archivedAt":archived.then_some("2026-10-22T00:00:00.000Z")})).unwrap();
        write_session_metadata(db.to_str().unwrap(), &row).unwrap();
    }
    json!({"db":db,"paths":[parent,child,root.join("root.jsonl.zst")]})
}
fn loader(paths: Vec<String>, archived: bool, events: Rc<RefCell<Vec<Value>>>) -> SessionListLoader {
    Rc::new(move |_| {
        events.borrow_mut().push(json!(["load"]));
        let sessions = paths
            .iter()
            .take(2)
            .enumerate()
            .filter(|(_, p)| Path::new(p).exists())
            .map(|(i, path)| SessionInfo {
                path: path.as_str().into(),
                id: i.to_string().into(),
                cwd: "/fixture".into(),
                name: Some(if i == 0 { "root" } else { "child" }.into()),
                is_archived: Some(archived),
                parent_session_path: None,
                created: 7258118400000.0,
                modified: 7258118400000.0,
                message_count: 2,
                first_message: "fixture".into(),
            })
            .collect();
        Box::pin(async move { Ok(sessions) })
    })
}
async fn native(request: &Value) -> Value {
    init_theme(Some(&"dark".into()), false).unwrap();
    pi_coding_agent::core::keybindings::KeybindingsManager::new(Vec::new(), None).install();
    let db = JsString::from(request["db"].as_str().unwrap());
    let paths: Vec<_> = request["paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_owned())
        .collect();
    let events = Rc::new(RefCell::new(Vec::<Value>::new()));
    let render = events.clone();
    let loader = loader(paths.clone(), request["archived"] == true, events.clone());
    let mut selector = SessionSelectorComponent::new(
        loader.clone(),
        loader,
        Rc::new(|_| {}),
        Rc::new(|| {}),
        Rc::new(|| {}),
        Rc::new(move || render.borrow_mut().push(json!(["render"]))),
        None,
        None,
        None,
        (request["withDb"] == true).then_some(db.clone()),
        None,
        None,
    );
    selector.load_current_sessions().await;
    let path = JsString::from(paths[0].as_str());
    if request["operation"] == "delete" {
        selector.delete_session(&path).await;
    } else {
        selector.archive_selected_session(&path).await;
    }
    let frames: Vec<_> = selector.render(90).iter().map(|s| s.units().to_vec()).collect();
    let files: Vec<_> = paths.iter().map(|p| Path::new(p).exists()).collect();
    let metadata: Vec<_> = paths
        .iter()
        .map(|p| {
            read_session_metadata(&db, JsString::from(p)).unwrap().map(|row| {
                json!([
                    row.session_path,
                    row.is_archived.unwrap_or(false),
                    row.parent_session_path
                ])
            })
        })
        .collect();
    json!({"frames":frames,"events":events.borrow().clone(),"files":files,"metadata":metadata})
}
#[tokio::test(flavor = "current_thread")]
#[ignore = "isolated child process"]
async fn mutation_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    for (operation, with_db, partial, archived) in [
        ("delete", true, false, false),
        ("delete", true, true, false),
        ("archive", true, false, false),
        ("archive", false, false, false),
        ("archive", true, false, true),
    ] {
        let root = temp.path().join(format!("{operation}-{with_db}-{partial}-{archived}"));
        let mut request = setup(&root, partial, archived);
        request["operation"] = json!(operation);
        request["withDb"] = json!(with_db);
        request["archived"] = json!(archived);
        let mut oracle = oracle::TsOracle::spawn("session-selector-mutation-oracle.mts", "packages/coding-agent/src");
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "{expected}");
        drop(oracle);
        std::fs::remove_dir_all(&root).unwrap();
        setup(&root, partial, archived);
        let actual = native(&request).await;
        for field in ["events", "files", "metadata"] {
            assert_eq!(
                actual[field], expected["output"][field],
                "{field} {operation} db{with_db} partial{partial} archived{archived}"
            );
        }
        for (line, (a, e)) in actual["frames"]
            .as_array()
            .unwrap()
            .iter()
            .zip(expected["output"]["frames"].as_array().unwrap())
            .enumerate()
        {
            assert_eq!(
                a, e,
                "frame{line} {operation} db{with_db} partial{partial} archived{archived}"
            );
        }
        assert_eq!(
            actual, expected["output"],
            "{operation} db{with_db} partial{partial} archived{archived}"
        );
        if operation == "delete" && !partial {
            assert_eq!(actual["files"], json!([false, false, false]));
            assert_eq!(actual["metadata"], json!([null, null, null]));
        }
        if operation == "archive" && with_db && !archived {
            assert_eq!(actual["files"], json!([false, true, true]));
        }
    }
}
