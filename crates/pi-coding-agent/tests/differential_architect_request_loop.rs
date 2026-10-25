//! Differential: `architect/main.ts` cycle and request completion against the pinned TypeScript module.

use pi_js_compat::js_json::JsString;
#[path = "support/oracle.rs"]
mod oracle;

use std::sync::{Arc, Mutex};

use pi_agent_core::agent::AgentMessage;
use pi_coding_agent::architect::main::{SentArchitectMessage, complete_sent_architect_request, run_architect_cycle};
use pi_coding_agent::architect::observer::ArchitectObserver;
use pi_coding_agent::core::session_control_db::{PostArchitectRequestInput, post_architect_request};
use pi_coding_agent::core::session_manager::{NewSessionOptions, SessionManager};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

const SOURCE_MODULE: &str = "packages/coding-agent/src/architect/main.ts";

fn normalize_session_file(path: &JsString, dir: &str) -> Vec<Value> {
    let content = std::fs::read_to_string(path.to_utf8().expect("utf-8 test path")).expect("session file");
    content
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut value: Value = serde_json::from_str(line).expect("session line");
            normalize_entry(&mut value);
            normalize_strings(&mut value, dir);
            value
        })
        .collect()
}

fn normalize_strings(value: &mut Value, dir: &str) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize_strings(item, dir);
            }
        }
        Value::Object(object) => {
            for child in object.values_mut() {
                normalize_strings(child, dir);
            }
        }
        Value::String(text) => {
            *text = normalize_prompt(&text.replace(dir, "<dir>"));
        }
        _ => {}
    }
}

fn normalize_prompt(prompt: &JsString) -> JsString {
    let mut normalized = JsString::default();
    let chars = prompt.units();
    let mut index = 0;
    while index < chars.len() {
        if is_iso_timestamp_start(chars, index) {
            let mut end = index;
            while end < chars.len() && chars[end] != u16::from(b'Z') {
                end += 1;
            }
            if end < chars.len() {
                normalized.push_str("<timestamp>");
                index = end + 1;
                continue;
            }
        }
        if is_uuid_start(chars, index) {
            normalized.push_str("<uuid>");
            index += 36;
            continue;
        }
        normalized.push_unit(chars[index]);
        index += 1;
    }
    normalized
}

fn is_iso_timestamp_start(chars: &[u16], index: usize) -> bool {
    if index + 20 > chars.len() {
        return false;
    }
    let digits = |offset: usize| (u16::from(b'0')..=u16::from(b'9')).contains(&chars[index + offset]);
    digits(0)
        && digits(1)
        && digits(2)
        && digits(3)
        && chars[index + 4] == u16::from(b'-')
        && digits(5)
        && digits(6)
        && chars[index + 7] == u16::from(b'-')
        && digits(8)
        && digits(9)
        && chars[index + 10] == u16::from(b'T')
        && digits(11)
        && digits(12)
        && chars[index + 13] == u16::from(b':')
}

fn is_uuid_start(chars: &[u16], index: usize) -> bool {
    if index + 36 > chars.len() {
        return false;
    }
    for (offset, character) in chars[index..index + 36].iter().enumerate() {
        let separator = matches!(offset, 8 | 13 | 18 | 23);
        if separator {
            if *character != u16::from(b'-') {
                return false;
            }
        } else if !(u16::from(b'0')..=u16::from(b'9')).contains(character)
            && !(u16::from(b'a')..=u16::from(b'f')).contains(character)
            && !(u16::from(b'A')..=u16::from(b'F')).contains(character)
        {
            return false;
        }
    }
    true
}

fn normalize_entry(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize_entry(item);
            }
        }
        Value::Object(object) => {
            for (key, child) in object.iter_mut() {
                if key == "id" || key == "parentId" {
                    *child = Value::String("<id>".to_owned().into());
                } else if key == "timestamp" {
                    *child = Value::String("<timestamp>".to_owned().into());
                } else if key == "cwd" {
                    *child = Value::String("<cwd>".to_owned().into());
                } else {
                    normalize_entry(child);
                }
            }
        }
        _ => {}
    }
}

async fn run_rust(dir: &str) -> Value {
    let control_db_path = format!("{dir}/control.sqlite");
    let session_dir = format!("{dir}/sessions");
    let manager = SessionManager::create(
        &JsString::from(dir),
        Some(&JsString::from(session_dir)),
        Some(NewSessionOptions {
            id: Some("architect".to_owned().into()),
            ..Default::default()
        }),
    )
    .expect("session manager");
    let session_file = manager.get_session_file().expect("session file").to_owned();
    let manager = Arc::new(Mutex::new(manager));
    post_architect_request(
        &control_db_path,
        &PostArchitectRequestInput {
            sender_session_id: "main-session".to_owned().into(),
            project_cwd: dir.to_owned().into(),
            body: "review this design".to_owned().into(),
        },
    )
    .expect("post request");
    let observer = Arc::new(ArchitectObserver::new(&JsString::from(&control_db_path), None).unwrap());
    let prompt_manager = Arc::clone(&manager);
    let prompt_text = Arc::new(Mutex::new(JsString::default()));
    let captured_prompt = Arc::clone(&prompt_text);
    run_architect_cycle(Arc::clone(&observer), move |content: JsString| {
        *captured_prompt.lock().unwrap() = content.clone();
        let manager = Arc::clone(&prompt_manager);
        async move {
            let message: AgentMessage = pi_js_compat::js_json::from_value(&json!({
                "role": "assistant",
                "content": [{"type": "text", "text": content}],
                "api": "faux",
                "provider": "faux",
                "model": "faux",
                "usage": {
                    "input": 0,
                    "output": 0,
                    "cacheRead": 0,
                    "cacheWrite": 0,
                    "totalTokens": 0,
                    "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}
                },
                "stopReason": "stop",
                "timestamp": 0
            }))
            .expect("user message");
            manager.lock().unwrap().append_message(message).expect("append");
            Ok(())
        }
    })
    .await
    .expect("architect cycle");
    complete_sent_architect_request(
        &*observer,
        &SentArchitectMessage {
            thread_id: Some("architect-request:1".to_owned().into()),
            to_agent_id: "main".to_owned().into(),
            to_session_id: "main-session".to_owned().into(),
        },
    )
    .expect("complete request");

    let connection = rusqlite::Connection::open(&control_db_path).expect("control db");
    let mut statement = connection
        .prepare("SELECT id, status, completed_at, sender_session_id FROM architect_requests ORDER BY id")
        .expect("select");
    let rows: Vec<Value> = statement
        .query_map([], |row| {
            let completed_at: Option<String> = row.get(2)?;
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "status": row.get::<_, String>(1)?,
                "completed_at": completed_at.map(|_| "<timestamp>"),
                "sender_session_id": row.get::<_, String>(3)?
            }))
        })
        .expect("rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("row values");
    json!({
        "controlRows": rows,
        "prompt": normalize_prompt(&prompt_text.lock().unwrap().replace(dir, "<dir>")),
        "session": normalize_session_file(&session_file, dir)
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn architect_request_cycle_matches_typescript() {
    let rust_dir = tempfile::tempdir().expect("rust temp");
    let ts_dir = tempfile::tempdir().expect("ts temp");
    let rust = run_rust(&rust_dir.path().display().to_string()).await;
    let mut oracle = oracle::TsOracle::spawn("coding-agent-architect-request-loop-oracle.ts", SOURCE_MODULE);
    let response = oracle.request(&json!({
        "id": 1,
        "operation": "architect-request-loop",
        "dir": ts_dir.path().display().to_string()
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let ts = &response["output"];
    assert_eq!(rust["controlRows"], ts["controlRows"]);
    assert_eq!(rust["prompt"], ts["prompt"]);
    assert_eq!(rust["session"], ts["session"]);
}
