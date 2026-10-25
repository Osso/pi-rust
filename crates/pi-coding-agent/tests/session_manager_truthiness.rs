#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_manager::{
    ForkEntrySelection, NewSessionOptions, SessionManager, SessionManagerError, find_most_recent_session,
    get_default_session_dir_path,
};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

struct Fixture {
    _root: tempfile::TempDir,
    cwd: JsString,
    agent: JsString,
    default_dir: JsString,
    source: JsString,
    custom_dir: JsString,
}

fn write_session(path: &JsString, cwd: &JsString, id: &str) {
    let header = json!({"type":"session","version":3,"id":id,"timestamp":"2025-01-01T00:00:00.000Z","cwd":cwd});
    let entry = json!({"type":"message","id":"entry-one","parentId":null,"timestamp":"2025-01-01T00:00:01.000Z","message":{"role":"user","content":"history","timestamp":1}});
    let alternate = json!({"type":"message","id":"entry-two","parentId":null,"timestamp":"2025-01-01T00:00:02.000Z","message":{"role":"user","content":"alternate branch","timestamp":2}});
    fs::write(encode_node_utf8(path), format!("{header}\n{entry}\n{alternate}\n")).unwrap();
}

fn create_fixture(seed_default: bool) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().join("workdir");
    fs::create_dir(&cwd).unwrap();
    let cwd = JsString::from(cwd.to_str().unwrap());
    let agent = JsString::from(root.path().join("agent").to_str().unwrap());
    pi_js_compat::process_env::set(&pi_coding_agent::config::env_agent_dir(), &encode_node_utf8(&agent));
    pi_js_compat::process_env::remove(&pi_coding_agent::config::env_session_dir());
    let default_dir = get_default_session_dir_path(&cwd, &agent).unwrap();
    if seed_default {
        fs::create_dir_all(encode_node_utf8(&default_dir)).unwrap();
        write_session(&(default_dir.clone() + "/fixture.jsonl"), &cwd, "fixture-id");
    }
    let source = JsString::from(root.path().join("source.jsonl").to_str().unwrap());
    write_session(&source, &cwd, "source-id");
    let custom_dir = JsString::from(root.path().join("custom").to_str().unwrap());
    Fixture {
        _root: root,
        cwd,
        agent,
        default_dir,
        source,
        custom_dir,
    }
}

fn read_snapshot(manager: &SessionManager) -> Value {
    let file = manager.get_session_file();
    let dir = manager.get_session_dir();
    json!({
        "cwd":manager.get_cwd().units(),"dir":dir.units(),
        "fileDirectory":file.map(|file|pi_coding_agent::utils::paths::node_dirname(file).units().to_vec()),
        "fileExists":file.is_some_and(|file|Path::new(&encode_node_utf8(file)).exists()),
        "dirExists":Path::new(&encode_node_utf8(dir)).exists(),
        "id":manager.get_session_id(),
        "entries":manager.get_entries().iter().map(|entry|entry.id().cloned()).collect::<Vec<_>>(),
    })
}

fn run_native(case: &str, fixture: &Fixture, dir: &JsString) -> Result<Value, SessionManagerError> {
    let empty = JsString::default();
    let options = || {
        Some(NewSessionOptions {
            id: Some("created-id".into()),
            ..Default::default()
        })
    };
    match case {
        "create" => Ok(read_snapshot(&SessionManager::create(
            &fixture.cwd,
            Some(dir),
            options(),
        )?)),
        "open" | "open-leading-blank" => Ok(read_snapshot(&SessionManager::open(
            &fixture.source,
            Some(dir),
            (case == "open").then_some(&empty),
        )?)),
        "readonly" => Ok(read_snapshot(&SessionManager::open_read_only(
            &fixture.source,
            Some(dir),
        )?)),
        "continue" => Ok(read_snapshot(&SessionManager::continue_recent(
            &fixture.cwd,
            Some(dir),
        )?)),
        "set-file" => {
            let mut manager = SessionManager::in_memory(Some(&fixture.cwd), options())?;
            manager.set_session_file(&fixture.source)?;
            Ok(read_snapshot(&manager))
        }
        "fork" | "fork-empty-from" => {
            let selection = (case == "fork-empty-from").then_some(ForkEntrySelection::From(empty));
            Ok(read_snapshot(&SessionManager::fork_from(
                &fixture.source,
                &fixture.cwd,
                Some(dir),
                options(),
                selection,
            )?))
        }
        "list" | "list-missing-root" | "list-empty-db" => {
            let database = (case == "list-empty-db").then_some(&empty);
            let sessions = SessionManager::list(&fixture.cwd, Some(dir), None, database)?;
            let ids: Vec<_> = sessions.into_iter().map(|session| session.id).collect();
            if case == "list-missing-root" {
                Ok(json!({"value":ids,"directoryExists":Path::new(&encode_node_utf8(&fixture.default_dir)).exists()}))
            } else {
                Ok(json!(ids))
            }
        }
        "list-all" => Ok(json!(
            SessionManager::list_all(Some(dir), None, None)?
                .into_iter()
                .map(|session| session.id)
                .collect::<Vec<_>>()
        )),
        "list-archived" => Ok(json!(
            SessionManager::list_archived(Some(dir), Some(&empty))?
                .into_iter()
                .map(|session| session.id)
                .collect::<Vec<_>>()
        )),
        "recent" => Ok(json!(
            find_most_recent_session(&fixture.default_dir, Some(&empty))?.map(|path| path.units().to_vec())
        )),
        _ => panic!("unsupported fixture case: {case}"),
    }
}

#[test]
fn session_manager_truthiness_matches_frozen_for_every_discovered_site() {
    let original_agent = pi_js_compat::process_env::get(&pi_coding_agent::config::env_agent_dir());
    let original_sessions = pi_js_compat::process_env::get(&pi_coding_agent::config::env_session_dir());
    let mut source = oracle::TsOracle::spawn("session-manager-truthiness-oracle.mts", "packages/coding-agent/src");
    let mut failures = Vec::new();
    for case in [
        "create",
        "open",
        "readonly",
        "continue",
        "fork",
        "fork-empty-from",
        "list",
        "list-missing-root",
        "list-empty-db",
        "list-all",
        "list-archived",
        "recent",
        "set-file",
        "open-leading-blank",
    ] {
        let fixture = create_fixture(matches!(
            case,
            "continue" | "list" | "list-empty-db" | "list-all" | "recent"
        ));
        if case == "set-file" {
            write_session(&fixture.source, &JsString::from("/different"), "source-id");
        }
        if case == "open-leading-blank" {
            let content = fs::read_to_string(encode_node_utf8(&fixture.source)).unwrap();
            fs::write(encode_node_utf8(&fixture.source), format!("\n  \n{content}")).unwrap();
        }
        let dir = match case {
            "fork-empty-from" => fixture.custom_dir.clone(),
            "list-empty-db" => fixture.default_dir.clone(),
            "list-archived" => "file://not-local/forbidden".into(),
            _ => JsString::default(),
        };
        let operation = match case {
            "open-leading-blank" => "open",
            "fork-empty-from" => "fork",
            "list-missing-root" | "list-empty-db" => "list",
            other => other,
        };
        let mut request = json!({"operation":operation,"agentDir":fixture.agent,"cwd":fixture.cwd.units(),"sessionDir":dir.units(),"controlDbPath":if matches!(case,"list-empty-db"|"list-archived"){json!([])}else{Value::Null},"path":if case=="recent" {&fixture.default_dir}else{&fixture.source},"id":"created-id","queryCwd":[],"cwdOverride":[]});
        if case == "fork-empty-from" {
            request["from"] = json!([]);
        }
        if case == "open-leading-blank" {
            request["cwdOverride"] = Value::Null;
        }
        if case == "list-missing-root" {
            request["inspectDir"] = json!(fixture.default_dir);
        }
        let expected = source.request(&request);
        assert_eq!(expected["ok"], true, "{case}, source: {expected}");
        if matches!(case, "list" | "list-empty-db" | "list-all") {
            assert_eq!(
                expected["output"],
                json!(["fixture-id"]),
                "source fixture must contain the requested session"
            );
        }
        let actual = match run_native(case, &fixture, &dir) {
            Ok(output) => json!({"ok":true,"output":output}),
            Err(error) => {
                let error = pi_ai::request::CallbackError::from(error);
                json!({"ok":false,"error":{"name":error.name,"message":error.message.units()}})
            }
        };
        if actual != expected {
            failures.push(format!("{case}: native={actual}; frozen={expected}"));
        }
    }
    for (key, original) in [
        (pi_coding_agent::config::env_agent_dir(), original_agent),
        (pi_coding_agent::config::env_session_dir(), original_sessions),
    ] {
        match original {
            Some(value) => pi_js_compat::process_env::set(&key, &value),
            None => pi_js_compat::process_env::remove(&key),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
