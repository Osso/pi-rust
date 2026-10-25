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
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};

struct Ui {
    trace: Mutex<Vec<Value>>,
    theme: Arc<Theme>,
}
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Result<Arc<Theme>, pi_ai::request::CallbackError> {
        Ok(self.theme.clone())
    }
    fn set_status(
        &self,
        key: &JsString,
        text: Option<pi_coding_agent::core::footer_data_provider::ExtensionStatus>,
    ) -> Result<(), pi_ai::request::CallbackError> {
        let text = text.map(|text| text.evaluate(None)).transpose()?.flatten();
        self.trace.lock().unwrap().push(json!(["status", key, text]));
        Ok(())
    }
    fn notify(
        &self,
        text: &JsString,
        level: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.trace.lock().unwrap().push(json!(["notify", text, level]));
        Ok(())
    }
}
fn seed(cwd: &Path, remote: &Path) {
    for path in [cwd, remote] {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("read.txt"), "first\nsecond\nthird").unwrap();
        std::fs::write(path.join("edit.txt"), "before\n").unwrap();
        std::fs::write(path.join("quote \" λ.txt"), "quoted").unwrap();
        std::fs::write(path.join("invalid.txt"), [0xff, b'x']).unwrap();
        std::fs::write(path.join("mime-error.txt"), "mime probe failed; read as text").unwrap();
        let png = pi_js_compat::base64::buffer_from_base64(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAX+XDSwAAAABJRU5ErkJggg==",
        );
        std::fs::write(path.join("image.png"), png).unwrap();
    }
}
async fn native(request: &Value) -> Value {
    let cwd = request["cwd"].as_str().unwrap();
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(&(cwd)))).unwrap();
    let runtime = create_extension_runtime();
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::ssh_extension(api);
        Box::pin(async move { result })
    });
    let extension = load_extension_from_factory(
        factory,
        &JsString::from(cwd),
        support::event_bus(),
        runtime.clone(),
        &JsString::from("<example:ssh>"),
    )
    .await
    .unwrap();
    let model_registry = pi_coding_agent::core::model_registry::ModelRegistry::in_memory(
        pi_coding_agent::core::auth_storage::AuthStorage::in_memory(pi_js_compat::js_json::JsObject::new()),
    )
    .unwrap();
    let mut runner = support::runner_for(vec![extension.clone()], cwd, runtime, Arc::new(model_registry));
    runner
        .bind_core(support::actions(), support::context_actions(), None)
        .unwrap();
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
    let errors = Arc::new(Mutex::new(Vec::<JsString>::new()));
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
        JsValue::from(json!({"path":"read.txt"})),
        None,
        None,
        Some(runner.create_context()),
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
        .emit_before_agent_start(
            &JsString::from("prompt"),
            None,
            &JsString::from(&prompt),
            BuildSystemPromptOptions::default(),
        )
        .await;
    let mut results = Vec::new();
    for call in request["calls"].as_array().unwrap() {
        let tool = runner.get_tool_definition(call["tool"].as_str().unwrap()).unwrap();
        results.push(
            match (tool.execute)(
                "call".into(),
                JsValue::from(call["params"].clone()),
                None,
                None,
                Some(runner.create_context()),
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
    let user = run_user_operations(ops, cwd).await;
    json!({"metadata":metadata,"before":before,"trace":trace,"startError":errors.lock().unwrap().first(),"prompt":changed.and_then(|r|r.system_prompt),"results":results,"user":user})
}
async fn run_user_operations(ops: Option<UserBashEventResult>, cwd: &str) -> Vec<Value> {
    let Some(ops) = ops.and_then(|result| result.operations) else {
        return Vec::new();
    };
    let mut user = Vec::new();
    for mode in ["success", "exit", "timeout", "overflow", "huge", "abort", "preabort"] {
        let output = Arc::new(Mutex::new(Vec::new()));
        let data = output.clone();
        let controller_token = pi_js_compat::abort::AbortController::new();
        let token = controller_token.signal();
        let controller_cancel = controller_token.clone();
        if mode == "preabort" {
            controller_token.abort(None);
        }
        let command = match mode {
            "timeout" | "overflow" | "huge" => "exec sleep 30",
            "abort" => {
                "exec python3 -c 'import os,signal,time; signal.signal(signal.SIGTERM,lambda *_:(os.write(1,b\"terminated\\n\"),os._exit(0))); os.write(1,b\"ready\\n\"); time.sleep(30)'"
            }
            "exit" => "printf code; exit 7",
            _ => "printf 'user output'",
        };
        let command = JsString::from(command);
        let cwd = JsString::from(cwd);
        let execution = ops.exec(
            &command,
            &cwd,
            BashExecOptions {
                on_data: Arc::new(move |bytes| {
                    data.lock().unwrap().extend_from_slice(bytes);
                    if mode == "abort" {
                        controller_cancel.abort(None);
                    }
                    Ok(())
                }),
                signal: Some(token),
                timeout: match mode {
                    "timeout" => Some(1.0),
                    "overflow" => Some(2147483.648),
                    "huge" => Some(1e100),
                    _ => None,
                },
                env: Some(indexmap::IndexMap::new()),
                detach: None,
            },
        );
        let result = tokio::time::timeout(std::time::Duration::from_secs(3), execution)
            .await
            .expect("custom SSH operation must finish promptly");
        if mode == "overflow" || mode == "huge" {
            let expected = if mode == "huge" {
                "timeout:1e+100"
            } else {
                "timeout:2147483.648"
            };
            assert_eq!(result.as_ref().unwrap_err().message, expected);
        }
        let output = String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
        user.push(match result {
            Ok(r) => json!({"mode":mode,"output":output,"result":{"exitCode":r.exit_code}}),
            Err(e) => json!({"mode":mode,"output":output,"error":e.message}),
        });
    }
    user
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
        for program in [
            "node", "git", "cat", "file", "base64", "mkdir", "sleep", "python3", "bash",
        ] {
            let executable = std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|directory| directory.join(program))
                .find(|path| path.is_file())
                .expect(program);
            std::os::unix::fs::symlink(executable, bin.join(program)).unwrap();
        }
        std::fs::write(bin.join("ssh"), include_str!("support/fake-ssh.py")).unwrap();
        std::fs::set_permissions(bin.join("ssh"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "ssh_fixture_remote_matches_pinned_example", "--nocapture"])
            .env("SSH_FIXTURE_CHILD", "1")
            .env("SSH_FIXTURE_ROOT", temp.path())
            .env("SSH_FIXTURE_LOG", temp.path().join("ssh.log"))
            .env("SSH_FIXTURE_REMOTE", temp.path().join("remote"))
            .env("PATH", &bin)
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
    for (index, flag) in [
        Value::Null,
        json!(""),
        json!("fixture"),
        json!(format!("fixture:{}", remote.display())),
        json!(format!("fixture:{}:ignored", remote.display())),
        json!("bad"),
        json!(format!("fixture$&:{}", remote.display())),
        json!(format!("fixture:{}/$&", remote.display())),
        json!("fixture"),
    ]
    .into_iter()
    .enumerate()
    {
        let spawn_missing = index == 8;
        if spawn_missing {
            std::fs::rename(root.join("bin/ssh"), root.join("ssh-disabled")).unwrap();
        }
        seed(&cwd, &remote);
        std::fs::write(&log, "").unwrap();
        let projected = String::from_utf16_lossy(&[0xd800]);
        assert_eq!(projected, "\u{fffd}");
        let request = json!({"cwd":cwd,"flag":flag,"calls":[
            {"tool":"read","params":{"path":"read.txt","offset":2,"limit":1}},
            {"tool":"read","params":{"path":"quote \" λ.txt"}},
            {"tool":"read","params":{"path":"invalid.txt"}},
            {"tool":"edit","params":{"path":"edit.txt","edits":[{"oldText":"before","newText":"after"}]}},
            {"tool":"write","params":{"path":"new.txt","content":format!("written\nλ🚀{projected}")}},
            {"tool":"bash","params":{"command":"printf stdout"}},
            {"tool":"bash","params":{"command":"printf stderr >&2; exit 7"}},
            {"tool":"read","params":{"path":"fail.txt"}},
            {"tool":"write","params":{"path":"fail.txt","content":"unwritten"}},
            {"tool":"edit","params":{"path":"fail.txt","edits":[{"oldText":"x","newText":"y"}]}},
            {"tool":"read","params":{"path":"image.png"}},
            {"tool":"read","params":{"path":"mime-error.txt"}},
            {"tool":"write","params":{"path":"fail-dir/new.txt","content":"unwritten"}}
        ]});
        let actual = native(&request).await;
        let actual_files = snapshot(&cwd, &remote);
        let actual_log = std::fs::read_to_string(&log).unwrap();
        // Exercise the real pi_tui renderer, not just a mock UI transcript.
        let (terminal, term) = virtual_terminal::concrete(100, 24);
        let tui = pi_tui::tui::TuiRenderer::new(term);
        let message = actual["results"][7]["error"].as_str().unwrap();
        use pi_coding_agent::modes::interactive::components::tool_execution::{
            ToolExecutionComponent, ToolExecutionOptions, ToolExecutionResult,
        };
        use pi_coding_agent::modes::interactive::theme::{REQUIRED_COLORS, set_theme_instance};
        let colors = REQUIRED_COLORS
            .iter()
            .map(|key| (JsString::from(*key), ColorValue::Text("#abcdef".into())))
            .collect::<Vec<_>>();
        set_theme_instance(Arc::new(
            Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap(),
        ));
        let execution = ToolExecutionComponent::new(
            "read",
            "call",
            JsValue::from(json!({"path":"fail.txt"})),
            ToolExecutionOptions::default(),
            None,
            Arc::new(|| {}),
            cwd.to_str().unwrap(),
        );
        execution.borrow_mut().update_result(
            ToolExecutionResult {
                content: vec![pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new(
                    message,
                ))],
                details: None,
                is_error: true,
            },
            false,
            None,
        );
        let component: pi_tui::tui::ComponentRef = execution;
        pi_tui::tui::TuiExt::add_child(tui.as_ref(), component);
        pi_tui::tui::TuiExt::request_render(tui.as_ref(), false);
        pi_tui::tui::TuiExt::render_now(tui.as_ref(), false).unwrap();
        assert!(terminal.borrow().viewport().join("\n").contains(
            if flag.is_null() || flag == "" || flag == "bad" || spawn_missing {
                "ENOENT"
            } else {
                "SSH failed (7): scripted failure"
            }
        ));
        pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(&(&root))))
            .unwrap();
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
        assert_eq!(
            pi_js_compat::js_json::to_value(&actual).unwrap(),
            pi_js_compat::js_json::to_value(&expected).unwrap(),
            "flag {flag}"
        );
        assert_eq!(actual_files, snapshot(&cwd, &remote), "side effects {flag}");
        assert_eq!(
            actual_log,
            std::fs::read_to_string(&log).unwrap(),
            "SSH argv/stdin {flag}"
        );
        if spawn_missing {
            assert_eq!(actual["startError"], "spawn ssh ENOENT");
            std::fs::rename(root.join("ssh-disabled"), root.join("bin/ssh")).unwrap();
        }
        for dir in [&cwd, &remote] {
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
