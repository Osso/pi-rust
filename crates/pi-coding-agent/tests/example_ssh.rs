#[path = "../examples/extensions/ssh.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;
#[path = "../../pi-tui/tests/support/virtual_terminal.rs"]
mod virtual_terminal;

use pi_coding_agent::{
    core::{extensions::*, system_prompt::BuildSystemPromptOptions, tools::bash::BashExecOptions},
    modes::interactive::theme::{ColorMode, ColorValue, Theme},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

struct Ui {
    trace: Mutex<Vec<Value>>,
    theme: Arc<Theme>,
}
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Arc<Theme> {
        self.theme.clone()
    }
    fn set_status(&self, key: &str, text: Option<&str>) {
        self.trace.lock().unwrap().push(json!([
            "status",
            key,
            text.map(pi_coding_agent::utils::ansi::strip_ansi)
        ]));
    }
    fn notify(&self, text: &str, level: Option<NotifyType>) {
        self.trace.lock().unwrap().push(json!(["notify", text, level]));
    }
}
fn seed(cwd: &Path, remote: &Path) {
    for path in [cwd, remote] {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("read.txt"), "first\nsecond\nthird").unwrap();
        std::fs::write(path.join("edit.txt"), "before\n").unwrap();
        std::fs::write(path.join("quote \" λ.txt"), "quoted").unwrap();
        std::fs::write(path.join("invalid.txt"), [0xff, b'x']).unwrap();
    }
}
async fn native(request: &Value) -> Value {
    let cwd = request["cwd"].as_str().unwrap();
    std::env::set_current_dir(cwd).unwrap();
    let runtime = create_extension_runtime();
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::ssh_extension(api);
        Box::pin(async move { result })
    });
    let extension = load_extension_from_factory(factory, cwd, support::event_bus(), runtime.clone(), "<example:ssh>")
        .await
        .unwrap();
    let model_runtime = pi_coding_agent::core::model_runtime::ModelRuntime::create(
        pi_coding_agent::core::model_runtime::CreateModelRuntimeOptions {
            credentials: Some(Arc::new(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
                Default::default(),
            ))),
            models_path: Some(None),
            allow_model_network: false,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let mut runner = support::runner_for(
        vec![extension.clone()],
        cwd,
        runtime,
        Arc::new(pi_coding_agent::core::model_registry::ModelRegistry::new(model_runtime)),
    );
    runner.bind_core(support::actions(), support::context_actions(), None);
    let ui = Arc::new(Ui {
        trace: Mutex::default(),
        theme: Arc::new(
            Theme::new(
                &[("accent".into(), ColorValue::Text("#abcdef".into()))],
                &[],
                ColorMode::Truecolor,
                None,
                None,
            )
            .unwrap(),
        ),
    });
    runner.set_ui_context(Some(ui.clone()), ExtensionMode::Tui);
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured = errors.clone();
    runner.on_error(Arc::new(move |error| {
        captured.lock().unwrap().push(error.error.clone())
    }));
    let metadata = extension
        .read()
        .tools
        .values()
        .map(|tool| {
            let t = &tool.definition;
            json!({"name":t.name,"label":t.label,"description":t.description,"parameters":t.parameters})
        })
        .collect::<Vec<_>>();
    let read = runner.get_tool_definition("read").unwrap();
    let before = (read.execute)(
        "before".into(),
        json!({"path":"read.txt"}),
        None,
        None,
        runner.create_context(),
    )
    .await
    .unwrap();
    if let Some(flag) = request["flag"].as_str() {
        runner.set_flag_value("ssh", FlagValue::String(flag.into()));
    }
    let flags = runner.get_flags();
    let f = &flags["ssh"];
    let mut trace = vec![json!(["flag",f.name,{"description":f.description,"type":"string"}])];
    runner
        .emit(&mut ExtensionEvent::SessionStart(SessionStartEvent {
            reason: SessionStartReason::Startup,
            previous_session_file: None,
        }))
        .await;
    trace.extend(ui.trace.lock().unwrap().clone());
    let prompt = format!("Current working directory: {cwd}\nCurrent working directory: {cwd}");
    let changed = runner
        .emit_before_agent_start("prompt", None, &prompt, BuildSystemPromptOptions::default())
        .await;
    let mut results = Vec::new();
    for call in request["calls"].as_array().unwrap() {
        let tool = runner.get_tool_definition(call["tool"].as_str().unwrap()).unwrap();
        results.push(
            match (tool.execute)(
                "call".into(),
                call["params"].clone(),
                None,
                None,
                runner.create_context(),
            )
            .await
            {
                Ok(result) => json!({"result":result}),
                Err(e) => json!({"error":e.message}),
            },
        );
    }
    let ops = runner
        .emit_user_bash(&UserBashEvent {
            command: "ignored".into(),
            exclude_from_context: false,
            cwd: cwd.into(),
        })
        .await;
    let mut user = Vec::new();
    if let Some(ops) = ops.and_then(|o| o.operations) {
        for mode in ["success", "exit", "timeout", "abort"] {
            let output = Arc::new(Mutex::new(Vec::new()));
            let data = output.clone();
            let token = CancellationToken::new();
            let cancel = token.clone();
            let command = match mode {
                "timeout" => "exec sleep 30",
                "abort" => "printf ready; exec sleep 30",
                "exit" => "printf code; exit 7",
                _ => "printf 'user output'",
            };
            let result = ops
                .exec(
                    command,
                    cwd,
                    BashExecOptions {
                        on_data: Arc::new(move |bytes| {
                            data.lock().unwrap().extend_from_slice(bytes);
                            if mode == "abort" {
                                cancel.cancel();
                            }
                        }),
                        signal: Some(token),
                        timeout: (mode == "timeout").then_some(0.03),
                        env: BTreeMap::new(),
                        detach: None,
                    },
                )
                .await;
            let output = String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
            user.push(match result {
                Ok(r) => json!({"mode":mode,"output":output,"result":{"exitCode":r.exit_code}}),
                Err(e) => json!({"mode":mode,"output":output,"error":e.message}),
            });
        }
    }
    json!({"metadata":metadata,"before":before,"trace":trace,"startError":errors.lock().unwrap().first(),"prompt":changed.and_then(|r|r.system_prompt),"results":results,"user":user})
}
fn snapshot(cwd: &Path, remote: &Path) -> Value {
    let read = |root: &Path| {
        ["edit.txt", "new.txt"]
            .into_iter()
            .map(|name| (name.to_owned(), std::fs::read_to_string(root.join(name)).ok()))
            .collect::<BTreeMap<_, _>>()
    };
    json!({"local":read(cwd),"remote":read(remote)})
}
#[tokio::test]
async fn ssh_fixture_remote_matches_pinned_example() {
    if std::env::var_os("SSH_FIXTURE_CHILD").is_none() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(bin.join("ssh"), include_str!("support/fake-ssh.py")).unwrap();
        std::fs::set_permissions(bin.join("ssh"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "ssh_fixture_remote_matches_pinned_example", "--nocapture"])
            .env("SSH_FIXTURE_CHILD", "1")
            .env("SSH_FIXTURE_ROOT", temp.path())
            .env("SSH_FIXTURE_LOG", temp.path().join("ssh.log"))
            .env("SSH_FIXTURE_REMOTE", temp.path().join("remote"))
            .env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let root = std::path::PathBuf::from(std::env::var("SSH_FIXTURE_ROOT").unwrap());
    let cwd = root.join("local");
    let remote = root.join("remote");
    let log = root.join("ssh.log");
    for flag in [
        Value::Null,
        json!(""),
        json!("fixture"),
        json!(format!("fixture:{}", remote.display())),
        json!(format!("fixture:{}:ignored", remote.display())),
        json!("bad"),
    ] {
        seed(&cwd, &remote);
        std::fs::write(&log, "").unwrap();
        let request = json!({"cwd":cwd,"flag":flag,"calls":[
            {"tool":"read","params":{"path":"read.txt","offset":2,"limit":1}},
            {"tool":"read","params":{"path":"quote \" λ.txt"}},
            {"tool":"read","params":{"path":"invalid.txt"}},
            {"tool":"edit","params":{"path":"edit.txt","edits":[{"oldText":"before","newText":"after"}]}},
            {"tool":"write","params":{"path":"new.txt","content":"written\nλ🚀"}},
            {"tool":"bash","params":{"command":"printf stdout"}},
            {"tool":"bash","params":{"command":"printf stderr >&2; exit 7"}},
            {"tool":"read","params":{"path":"fail.txt"}},
            {"tool":"write","params":{"path":"fail.txt","content":"unwritten"}},
            {"tool":"edit","params":{"path":"fail.txt","edits":[{"oldText":"x","newText":"y"}]}}
        ]});
        let actual = native(&request).await;
        let actual_files = snapshot(&cwd, &remote);
        let actual_log = std::fs::read_to_string(&log).unwrap();
        // Exercise the real pi_tui renderer, not just a mock UI transcript.
        let (terminal, term) = virtual_terminal::concrete(100, 24);
        let tui = pi_tui::tui::TuiRenderer::new(term);
        let message = actual["results"][7]["error"].as_str().unwrap();
        let component: pi_tui::tui::ComponentRef =
            std::rc::Rc::new(std::cell::RefCell::new(pi_tui::Text::new(message, 0, 0)));
        pi_tui::tui::TuiExt::add_child(tui.as_ref(), component);
        pi_tui::tui::TuiExt::request_render(tui.as_ref(), false);
        pi_tui::tui::TuiExt::render_now(tui.as_ref(), false);
        assert!(
            terminal
                .borrow()
                .viewport()
                .join("\n")
                .contains(if flag.is_null() || flag == "" || flag == "bad" {
                    "ENOENT"
                } else {
                    "SSH failed (7): scripted failure"
                })
        );
        std::env::set_current_dir(&root).unwrap();
        for dir in [&cwd, &remote] {
            std::fs::remove_dir_all(dir).unwrap();
        }
        seed(&cwd, &remote);
        std::fs::write(&log, "").unwrap();
        let mut oracle = oracle::TsOracle::spawn(
            "example-ssh-oracle.ts",
            "packages/coding-agent/examples/extensions/ssh.ts",
        );
        let expected = oracle.request(&request);
        assert_eq!(actual, expected, "flag {flag}");
        assert_eq!(actual_files, snapshot(&cwd, &remote), "side effects {flag}");
        assert_eq!(
            actual_log,
            std::fs::read_to_string(&log).unwrap(),
            "SSH argv/stdin {flag}"
        );
        for dir in [&cwd, &remote] {
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
