//! Test-only KB namespace, resident identity, and durable advisory responder.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pi_ai::request::BoxFuture;
use pi_coding_agent::core::extensions::types::ContextTool;
use pi_coding_agent::core::resident_console_transport::{
    ResidentConsoleHost, ResidentConsoleIdentity, ResidentConsoleManagedBy, ResidentConsoleServer,
    ResidentConsoleServerOptions, ResidentConsoleService, ResidentConsoleSnapshot, Unsubscribe, probe_resident_console,
};
use pi_coding_agent::core::session_control_db::{
    SupervisorResponse, claim_next_supervisor_request, complete_supervisor_request, read_supervisor_request,
};
use pi_coding_agent::core::tools::ask_supervisor::AskSupervisorTool;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};

const KB: &str = "/syncthing/Sync/KB";
const ROOT_ENV: &str = "PI_ADVISORY_PRIVATE_ROOT";

pub fn run_isolated() {
    let root = create_private_fixture();
    let log = root.path().join("probe.log");
    let mut child = spawn_private_probe(&root, &log);
    wait_private_probe(&mut child, &log);
}

fn create_private_fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for relative in [
        "kb/memory/supervisor",
        "project",
        "tmp",
        "home",
        "config",
        "state",
        "data",
        "agent",
    ] {
        std::fs::create_dir_all(root.path().join(relative)).unwrap();
    }
    let cwd = pi_js_compat::node_fs::decode_node_path(root.path().join("project").as_os_str());
    let mapping = json!({"fixture-project": {"roots": [cwd]}});
    std::fs::write(
        root.path().join("kb/memory/supervisor/projects.json"),
        js_json::stringify_lossless(&mapping),
    )
    .unwrap();
    root
}

fn spawn_private_probe(root: &tempfile::TempDir, log: &Path) -> Child {
    let stdout = std::fs::File::create(log).unwrap();
    let mut command = Command::new("bwrap");
    command
        .args([
            "--die-with-parent",
            "--unshare-user",
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--tmpfs",
            "/syncthing",
            "--dir",
            "/syncthing/Sync",
            "--bind",
        ])
        .arg(root.path())
        .arg(root.path())
        .arg("--ro-bind")
        .arg(root.path().join("kb"))
        .arg(KB);
    for (name, relative) in [
        ("TMPDIR", "tmp"),
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_DATA_HOME", "data"),
        ("PI_CODING_AGENT_DIR", "agent"),
    ] {
        command.arg("--setenv").arg(name).arg(root.path().join(relative));
    }
    command
        .arg("--setenv")
        .arg(ROOT_ENV)
        .arg(root.path())
        .arg("--")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "isolated_advisory_probe", "--ignored", "--nocapture"])
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().unwrap())
        .stderr(stdout);
    command.spawn().unwrap()
}

fn wait_private_probe(child: &mut Child, log: &Path) {
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(30) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "private advisory fixture timed out: {}",
                std::fs::read_to_string(log).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        status.success(),
        "private advisory fixture failed: {}",
        std::fs::read_to_string(log).unwrap()
    );
}

fn read_private_root() -> PathBuf {
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("private KB fixture is required"));
    let suffix = format!("/{}/kb", root.file_name().unwrap().to_str().unwrap());
    let mounts = std::fs::read_to_string("/proc/self/mountinfo").unwrap();
    let private_bind = mounts.lines().any(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        fields.len() > 4 && fields[4] == KB && fields[3].ends_with(&suffix)
    });
    assert!(private_bind, "refusing to read KB without the private namespace bind");
    root
}

struct IdentityHost;
impl ResidentConsoleHost<JsValue, JsValue> for IdentityHost {
    fn get_snapshot(&self) -> ResidentConsoleSnapshot<JsValue> {
        ResidentConsoleSnapshot {
            service: ResidentConsoleService::Supervisor,
            session_id: "private-resident".into(),
            cwd: "/".into(),
            generation: 1,
            identity: Some(ResidentConsoleIdentity {
                version: pi_coding_agent::config::version().clone(),
                pid: std::process::id() as f64,
                executable: "private-identity-fixture".into(),
                entrypoint: None,
                instance_id: "private-fixture".into(),
                managed_by: ResidentConsoleManagedBy::External,
                ready: true,
            }),
            branch: vec![],
        }
    }
    fn enqueue_prompt(&self, _text: JsString, _id: JsString) -> BoxFuture<'static, Result<(), JsString>> {
        Box::pin(async { Err("identity-only test fixture".into()) })
    }
    fn subscribe(&self, _listener: Arc<dyn Fn(JsValue) + Send + Sync>) -> Unsubscribe {
        Box::new(|| {})
    }
}

async fn start_identity(path: &JsString) -> ResidentConsoleServer<JsValue, JsValue> {
    // A ready resident has already initialized its control DB; do not race first-open migrations.
    assert!(read_supervisor_request(path, 1).unwrap().is_none());
    let socket_path = path.clone() + ".supervisor-console.sock";
    let server = ResidentConsoleServer::new(ResidentConsoleServerOptions {
        socket_path: socket_path.clone(),
        service: ResidentConsoleService::Supervisor,
        host: Arc::new(IdentityHost),
    });
    server.start().await.unwrap();
    let probe = probe_resident_console::<JsValue>(socket_path, ResidentConsoleService::Supervisor)
        .await
        .unwrap();
    assert_eq!(&probe.identity.unwrap().version, pi_coding_agent::config::version());
    server
}

fn now_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as f64
}

async fn respond(path: JsString, response: SupervisorResponse) -> JsValue {
    let started = Instant::now();
    let token = JsString::from("private-responder");
    let request = loop {
        if let Some(request) = claim_next_supervisor_request(&path, &token).unwrap() {
            break request;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "advisory request was never posted"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    let db = rusqlite::Connection::open(js_json::encode_node_utf8(&path)).unwrap();
    let raw: String = db
        .query_row(
            "SELECT payload_json FROM supervisor_requests WHERE id=?",
            [request.id],
            |row| row.get(0),
        )
        .unwrap();
    if matches!(&response, SupervisorResponse::Approve { .. }) {
        // Exercise the tool's defensive response-kind branch using legacy/corrupt persisted state.
        // The normal completion API correctly rejects this kind for an advisory request.
        let response_json = js_json::stringify_lossless(&js_json::to_value(&response).unwrap());
        let changed = db
            .execute(
                "UPDATE supervisor_requests SET status='completed', response_json=? WHERE id=? AND claim_token=?",
                rusqlite::params![response_json, request.id, js_json::encode_node_utf8(&token)],
            )
            .unwrap();
        assert_eq!(changed, 1);
    } else {
        complete_supervisor_request(&path, request.id, &token, &js_json::to_value(&response).unwrap()).unwrap();
    }
    json!({"kind": request.kind, "sender": request.sender_session_id, "project": request.project_id,
        "payload": JsValue::Object(request.payload), "payloadJson": raw, "deadline": request.deadline_at})
}

struct Case {
    params: JsValue,
    response: SupervisorResponse,
    aborted: bool,
}
fn build_cases() -> Vec<Case> {
    let raw = JsString::from_utf16(vec![0xd800]);
    vec![
        Case {
            params: json!({"question":"Is this complete?"}),
            response: SupervisorResponse::Advisory {
                answer: "Continue.".into(),
            },
            aborted: false,
        },
        Case {
            params: json!({"question":JsString::from("  question-")+&raw+"  "}),
            response: SupervisorResponse::Advisory {
                answer: JsString::from(" answer-") + &raw,
            },
            aborted: false,
        },
        Case {
            params: json!({"question":"","context":""}),
            response: SupervisorResponse::Advisory { answer: "".into() },
            aborted: true,
        },
        Case {
            params: json!({"question": raw.clone()+"q".repeat(3999),"context":"é".repeat(8000)}),
            response: SupervisorResponse::Advisory {
                answer: "bounded".into(),
            },
            aborted: false,
        },
        Case {
            params: json!({"question":"error","context":raw.clone()}),
            response: SupervisorResponse::Error {
                reason: JsString::from("reason-") + &raw,
            },
            aborted: false,
        },
        Case {
            params: json!({"question":"other kind"}),
            response: SupervisorResponse::Approve {
                reason: "not advisory".into(),
            },
            aborted: false,
        },
    ]
}

async fn execute_native_advisory(path: &JsString, cwd: &JsString, case: &Case) -> JsValue {
    let updates = Arc::new(AtomicUsize::new(0));
    let observed = updates.clone();
    let on_update: pi_agent_core::AgentToolUpdateCallback = Arc::new(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
        Err(pi_agent_core::ToolError::new("advisory must not emit updates"))
    });
    let (_runner, context) = super::tool_context::TestContextBuilder::new(
        cwd.clone(),
        super::tool_context::in_memory_session(cwd.clone(), "main-session"),
    )
    .control_db_path(path.clone())
    .build()
    .await;
    let signal = case.aborted.then(|| {
        pi_js_compat::abort::AbortSignal::abort(Some(pi_js_compat::abort::AbortReason::Value(json!("ignored"))))
    });
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        AskSupervisorTool::new().execute(
            "advisory".into(),
            case.params.clone(),
            signal,
            Some(on_update),
            Some(context),
            None,
        ),
    )
    .await
    .unwrap();
    assert_eq!(updates.load(Ordering::SeqCst), 0);
    match result {
        Ok(result) => json!({"ok":true,"result":result,"updates":[]}),
        Err(error) => json!({"ok":false,"error":error.message,"updates":[]}),
    }
}

fn assert_request(record: &JsValue, case: &Case, before: f64, after: f64) {
    assert_eq!(record["kind"], json!("supervisor_advisory"));
    assert_eq!(record["sender"], json!("main-session"));
    assert_eq!(record["project"], json!("fixture-project"));
    assert_eq!(record["payload"], case.params);
    assert_eq!(record["payloadJson"], json!(js_json::stringify_lossless(&case.params)));
    let deadline = pi_js_compat::date::date_parse_utf16(record["deadline"].as_string().unwrap().units());
    assert!(
        deadline >= before + 180_000.0 && deadline <= after + 180_000.0,
        "wrong advisory deadline"
    );
}

pub async fn run_probe() {
    let root = read_private_root();
    let cwd = pi_js_compat::node_fs::decode_node_path(root.join("project").as_os_str());
    let mut oracle = super::oracle::TsOracle::spawn(
        "ask-supervisor-oracle.ts",
        "packages/coding-agent/src/core/tools/ask-supervisor.ts",
    );
    for (index, case) in build_cases().into_iter().enumerate() {
        eprintln!("advisory fixture case {index}: start identities");
        let native_db = pi_js_compat::node_fs::decode_node_path(root.join(format!("n{index}")).as_os_str());
        let source_db = pi_js_compat::node_fs::decode_node_path(root.join(format!("s{index}")).as_os_str());
        let native_server = start_identity(&native_db).await;
        let source_server = start_identity(&source_db).await;
        let probe = oracle.request(&json!({"id":index,"operation":"probe","controlDbPath":source_db}));
        assert_eq!(probe["ok"], json!(true), "{probe:?}");
        assert_eq!(probe["output"]["version"], json!(pi_coding_agent::config::version()));
        assert_eq!(
            probe["output"]["snapshot"]["identity"]["version"],
            probe["output"]["version"]
        );
        let source_responder = tokio::spawn(respond(source_db.clone(), case.response.clone()));
        eprintln!("advisory fixture case {index}: source request");
        let before_source = now_ms();
        let expected = oracle.request(&json!({"id":index,"controlDbPath":source_db,"cwd":cwd,"sender":"main-session","params":case.params,"aborted":case.aborted}));
        let after_source = now_ms();
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        eprintln!("advisory fixture case {index}: source returned");
        let source_record = source_responder.await.unwrap();
        assert_request(&source_record, &case, before_source, after_source);
        let native_responder = tokio::spawn(respond(native_db.clone(), case.response.clone()));
        eprintln!("advisory fixture case {index}: native request");
        let before_native = now_ms();
        let actual = execute_native_advisory(&native_db, &cwd, &case).await;
        let after_native = now_ms();
        eprintln!("advisory fixture case {index}: native returned");
        let native_record = native_responder.await.unwrap();
        assert_request(&native_record, &case, before_native, after_native);
        assert_eq!(actual, expected["output"], "advisory case {index}");
        if case.params["question"] == json!("Is this complete?") {
            assert_eq!(
                actual["result"]["content"],
                json!([{ "type": "text", "text": "Continue." }])
            );
            assert_eq!(actual["result"]["details"]["senderSessionId"], json!("main-session"));
            assert_eq!(actual["result"]["details"]["answer"], json!("Continue."));
        }
        eprintln!("advisory fixture case {index}: closing identities");
        native_server.close().await.unwrap();
        source_server.close().await.unwrap();
        assert!(!Path::new(&js_json::encode_node_utf8(&(native_db + ".supervisor-start.lock"))).exists());
        assert!(!Path::new(&js_json::encode_node_utf8(&(source_db + ".supervisor-start.lock"))).exists());
    }
}
