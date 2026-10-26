use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use pi_coding_agent::core::session_control_db::{
    CreateReminderInput, ReminderListStatus, create_reminder, list_reminders,
};
use serde_json::{Value, json};

struct Pi {
    child: Child,
    output: Receiver<Value>,
}

impl Drop for Pi {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Request {
    body: Value,
    reply: Sender<()>,
}

fn provider() -> (u16, Receiver<Request>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || serve(stream, tx));
        }
    });
    (port, rx)
}

fn serve(mut stream: TcpStream, tx: Sender<Request>) {
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    let (start, length) = loop {
        let n = stream.read(&mut buf).unwrap();
        if n == 0 {
            return;
        }
        bytes.extend_from_slice(&buf[..n]);
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
            let length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse::<usize>()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                break (end + 4, length);
            }
        }
    };
    let body = serde_json::from_slice(&bytes[start..start + length]).unwrap();
    let (reply, wait) = mpsc::channel();
    if tx.send(Request { body, reply }).is_err() || wait.recv().is_err() {
        return;
    }
    let body = concat!(
        "data: {\"id\":\"test\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"demo-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Noted.\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"test\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"demo-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n",
        "data: [DONE]\n\n"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn wait_for(mut predicate: impl FnMut() -> bool, timeout: Duration, description: &str) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out waiting for {description}");
}

fn entries(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn reminder_entries(path: &Path) -> Vec<Value> {
    entries(path)
        .into_iter()
        .filter(|v| v["type"] == "custom_message" && v["customType"] == "reminder")
        .collect()
}

fn start(work: &Path, session: &Path, env: &[(String, String)]) -> Pi {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args([
            "--mode",
            "rpc",
            "--session",
            session.to_str().unwrap(),
            "--model",
            "demo/demo-model",
            "--api-key",
            "test",
        ])
        .envs(env.iter().map(|(key, value)| (key, value)))
        .current_dir(work)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let (sender, output) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str(&line) {
                let _ = sender.send(value);
            }
        }
    });
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut stderr, &mut std::io::stderr());
    });
    Pi { child, output }
}

#[test]
fn queued_reminder_survives_kill_and_restart() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let work = root.join("work");
    let agent = root.join("agent");
    let state = root.join("state");
    let home = root.join("home");
    let bin = root.join("bin");
    for dir in [&work, &agent, &state, &home, &bin] {
        fs::create_dir_all(dir).unwrap();
    }
    let log = root.join("alerts");
    let notify = bin.join("notify-send");
    fs::write(
        &notify,
        format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()),
    )
    .unwrap();
    fs::set_permissions(&notify, fs::Permissions::from_mode(0o755)).unwrap();
    let (port, requests) = provider();
    fs::write(agent.join("models.json"), json!({"providers":{"demo":{"baseUrl":format!("http://127.0.0.1:{port}/v1"),"api":"openai-completions","models":[{"id":"demo-model","name":"Demo","reasoning":false,"input":["text"],"contextWindow":64000,"maxTokens":4096}]}}}).to_string()).unwrap();
    let session = root.join("session.jsonl");
    let session_id = "reminder-crash-test";
    fs::write(
        &session,
        format!(
            "{}\n",
            json!({"type":"session","version":3,"id":session_id,"timestamp":"2026-01-01T00:00:00.000Z","cwd":work})
        ),
    )
    .unwrap();
    fs::write(
        agent.join("settings.json"),
        json!({"reminders":{"deliverySessionId":session_id}}).to_string(),
    )
    .unwrap();
    let env = vec![
        ("HOME".into(), home.display().to_string()),
        ("PI_CODING_AGENT_DIR".into(), agent.display().to_string()),
        ("PI_CODING_AGENT_STATE_DIR".into(), state.display().to_string()),
        ("PATH".into(), format!("{}:/usr/bin:/bin", bin.display())),
        ("PI_OFFLINE".into(), "1".into()),
        ("TERM".into(), "xterm-256color".into()),
        ("NO_COLOR".into(), "1".into()),
    ];
    let db = state.join("control.sqlite");
    let db_string = db.to_string_lossy().into_owned();
    let mut child = start(&work, &session, &env);
    writeln!(
        child.child.stdin.as_mut().unwrap(),
        "{}",
        json!({"type":"prompt","message":"Long task"})
    )
    .unwrap();
    let busy = requests
        .recv_timeout(Duration::from_secs(30))
        .expect("busy provider request");
    assert!(busy.body.to_string().contains("Long task"));
    let reminder = create_reminder(
        db_string.as_str(),
        &CreateReminderInput {
            title: "Security advisory recheck".into(),
            notes: None,
            project_path: None,
            due_at: "2020-01-01T00:00:00.000Z".into(),
        },
    )
    .unwrap();
    wait_for(
        || fs::read_to_string(&log).is_ok_and(|s| s.lines().count() == 1),
        Duration::from_secs(75),
        "first alert while busy",
    );
    assert_eq!(
        fs::read_to_string(&log).unwrap().trim(),
        format!("--app-name=pi Pi reminder #{} Security advisory recheck", reminder.id)
    );
    writeln!(
        child.child.stdin.as_mut().unwrap(),
        "{}",
        json!({"id":"queued-state","type":"get_state"})
    )
    .unwrap();
    let state = loop {
        let event = child
            .output
            .recv_timeout(Duration::from_secs(10))
            .expect("get_state response");
        if event["id"] == "queued-state" {
            break event;
        }
    };
    assert_eq!(state["data"]["sessionId"], session_id);
    assert_eq!(state["data"]["isStreaming"], true);
    child.child.kill().unwrap();
    child.child.wait().unwrap();
    drop(busy);
    assert!(reminder_entries(&session).is_empty());
    assert!(
        list_reminders(db_string.as_str(), ReminderListStatus::Open).unwrap()[0]
            .delivered_at
            .is_none()
    );
    let _child = start(&work, &session, &env);
    let resumed = requests
        .recv_timeout(Duration::from_secs(30))
        .expect("interrupted turn must resume first");
    let messages = resumed.body["messages"].as_array().unwrap();
    assert!(
        messages.last().unwrap().to_string().contains("Long task"),
        "resumed request: {}",
        resumed.body
    );
    resumed.reply.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let delivery = requests
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("reminder must trigger a turn");
        let is_reminder = delivery.body.to_string().contains("Security advisory recheck");
        delivery.reply.send(()).unwrap();
        if is_reminder {
            break;
        }
    }
    wait_for(
        || {
            list_reminders(db_string.as_str(), ReminderListStatus::Open).unwrap()[0]
                .delivered_at
                .is_some()
        },
        Duration::from_secs(10),
        "reminder acknowledgement",
    );
    let written = reminder_entries(&session);
    assert_eq!(written.len(), 1);
    assert!(written[0].to_string().contains("Security advisory recheck"));
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 2);
}
