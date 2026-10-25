#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_coding_agent::bundled_extensions::pyrun::{PyrunExtensionOptions, install_pyrun_extension};
use pi_coding_agent::core::extensions::ExtensionRunner;
use pi_coding_agent::core::extensions::types::RuntimeMailboxEvent;
use pi_js_compat::{js_json as json, process_env};
static ENVIRONMENT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn pyrun_registration_checks_command_without_parsing_args_and_routes_bridge() {
    let _environment = ENVIRONMENT.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_str().unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "pyrun-registration-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/index.ts",
    );
    for available in [false, true] {
        process_env::set(
            "PI_PYRUN_RUNNER_COMMAND",
            if available {
                "/bin/true"
            } else {
                "/missing-pyrun-runner"
            },
        );
        process_env::set(
            "PI_PYRUN_RUNNER_ARGS",
            "invalid JSON deliberately deferred until evaluation",
        );
        let runtime = support::runtime();
        let extension = support::extension(&runtime, cwd, "pyrun", |api| {
            install_pyrun_extension(api, PyrunExtensionOptions::default()).unwrap()
        })
        .await;
        let mut runner = ExtensionRunner::new(
            vec![extension],
            runtime,
            cwd,
            support::session_manager(cwd),
            support::model_registry().await,
            None,
        );
        runner
            .bind_core(support::actions(), support::context_actions(), None)
            .unwrap();
        let definition = runner.get_tool_definition("pyrun_eval");
        let actual = definition.map(|tool|json!({"name":tool.name,"description":tool.description,"promptSnippet":tool.prompt_snippet,"promptGuidelines":tool.prompt_guidelines}));
        let expected =
            oracle.request(&json!({"id":1,"command":if available { "/bin/true" } else { "/missing-pyrun-runner" }}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        assert_eq!(json!(actual), expected["output"]);
        if available {
            let body = json!({"protocol":"pyrun-bridge-v1","command":"request","requestId":"request-1","method":"models.scoped","params":null,"toolCallId":"call-1","replyTo":{"sessionId":"session","agentId":"job"},"identity":{"jobId":"job","owner":{"sessionId":"session","agentId":null},"processIdentity":{"pid":999999999,"startTimeTicks":7},"outputLabel":"Pyrun output"}});
            let message = json::from_value(&json!({"id":1,"status":"pending","updatedAt":"2026-01-01T00:00:00.000Z","body":json::stringify_lossless(&body),"createdAt":"2026-01-01T00:00:00.000Z","kind":"system","sender":{"sessionId":"session","agentId":"job"},"recipient":{"sessionId":"session","agentId":null},"storeRef":{"sessionPath":"/session.jsonl","messageId":"message"}})).unwrap();
            let error = runner
                .emit_runtime_mailbox(RuntimeMailboxEvent { message })
                .await
                .expect_err("bridge must reject a missing control database, not silently return unhandled");
            assert_eq!(error.message, "Detached Pyrun bridge requires the control database");
        }
    }
    process_env::set("PI_PYRUN_RUNNER_COMMAND", "/bin/true");
    process_env::set("PI_PYRUN_RUNNER_ARGS", "[]");
    let runtime = support::runtime();
    let extension = support::extension(&runtime, cwd, "pyrun-no-session", |api| {
        install_pyrun_extension(
            api,
            PyrunExtensionOptions {
                background_jobs_store: Some(std::sync::Arc::new(
                    pi_coding_agent::core::multi_agent_store::MultiAgentStore::new(Default::default()),
                )),
                detach_registry: Some(std::sync::Arc::new(
                    pi_coding_agent::core::tool_detach_registry::ToolDetachRegistry::new(),
                )),
                ..Default::default()
            },
        )
        .unwrap();
    })
    .await;
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        cwd,
        support::session_manager(cwd),
        support::model_registry().await,
        None,
    );
    runner
        .bind_core(support::actions(), support::context_actions(), None)
        .unwrap();
    let tool = runner.get_tool_definition("pyrun_eval").unwrap();
    let error = (tool.execute)(
        "call-1".into(),
        json!({"code":"1+1"}),
        None,
        None,
        Some(runner.create_context()),
    )
    .await
    .unwrap_err();
    assert!(
        error.message.starts_with("Pyrun runner exited"),
        "non-persisted stores must stay foreground: {:?}",
        error.message
    );
    process_env::clear();
}

#[tokio::test]
async fn pyrun_dispatcher_supervisor_and_active_tool_guards_match_frozen() {
    use pi_coding_agent::bundled_extensions::pyrun::{index::create_pyrun_pi_dispatcher, runner::PiRequest};
    use std::sync::{Arc, Mutex};
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_str().unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "pyrun-registration-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/index.ts",
    );
    for (standalone, method) in [
        (true, "agents.list"),
        (true, "messages.send"),
        (true, "sessions.resume"),
        (false, "agents.spawn"),
        (false, "agents.wait"),
        (false, "agents.attachSession"),
        (false, "agents.list"),
        (false, "agents.current"),
        (false, "agents.select"),
        (false, "messages.send"),
        (false, "custom"),
    ] {
        let runtime = support::runtime();
        let api = Arc::new(Mutex::new(None));
        let capture = api.clone();
        let extension = support::extension(&runtime, cwd, "dispatcher", move |api| {
            *capture.lock().unwrap() = Some(api.clone())
        })
        .await;
        let mut runner = ExtensionRunner::new(
            vec![extension],
            runtime,
            cwd,
            support::session_manager(cwd),
            support::model_registry().await,
            None,
        );
        let mut context = support::context_actions();
        context.get_no_supervisor = Some(Arc::new(move || standalone));
        runner.bind_core(support::actions(), context, None).unwrap();
        let dispatcher = create_pyrun_pi_dispatcher(
            &api.lock().unwrap().take().unwrap(),
            &PyrunExtensionOptions {
                pi_request_handlers: vec![
                    pi_coding_agent::bundled_extensions::pyrun::index::PyrunPiRequestHandler {
                        dispatch: Arc::new(|_, _, _, _| Box::pin(async { Ok(Some(json!("handled"))) })),
                        dispose: None,
                    },
                ],
                ..Default::default()
            },
        );
        let actual = match dispatcher(
            PiRequest {
                method: method.into(),
                params: json!(null),
            },
            runner.create_context(),
            None,
            None,
        )
        .await
        {
            Ok(result) => json!({"result":result}),
            Err(error) => json!({"error":error.message}),
        };
        let expected =
            oracle.request(&json!({"id":1,"operation":"dispatcher","method":method,"standalone":standalone}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        assert_eq!(actual, expected["output"], "{method}, standalone={standalone}");
    }
}

#[tokio::test]
async fn pyrun_custom_prompt_and_handler_disposal_match_frozen() {
    let _environment = ENVIRONMENT.lock().await;
    process_env::set("PI_PYRUN_RUNNER_COMMAND", "/bin/true");
    use pi_coding_agent::bundled_extensions::pyrun::index::{
        PyrunPiRequestHandler, PyrunToolDefinitionOptions, create_pyrun_tool_definition,
    };
    use std::sync::{Arc, Mutex};
    let mut oracle = oracle::TsOracle::spawn(
        "pyrun-registration-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/index.ts",
    );
    let options = json!({"promptSnippet":"","promptGuidelines":[]});
    let tool = create_pyrun_tool_definition(
        Arc::new(|_, _, _, _, _| Box::pin(async { unreachable!() })),
        pi_js_compat::js_json::from_value::<PyrunToolDefinitionOptions>(&options).unwrap(),
    );
    let expected = oracle.request(&json!({"id":1,"operation":"definition","options":options}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        json!({"promptSnippet":tool.prompt_snippet,"promptGuidelines":tool.prompt_guidelines}),
        expected["output"]
    );
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_str().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let disposed = events.clone();
    let runtime = support::runtime();
    let extension = support::extension(&runtime, cwd, "disposal", move |api| {
        install_pyrun_extension(
            api,
            PyrunExtensionOptions {
                pi_request_handlers: vec![PyrunPiRequestHandler {
                    dispatch: Arc::new(|_, _, _, _| Box::pin(async { Ok(None) })),
                    dispose: Some(Arc::new(move || {
                        disposed.lock().unwrap().push("disposed");
                        Ok(())
                    })),
                }],
                ..Default::default()
            },
        )
        .unwrap()
    })
    .await;
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        cwd,
        support::session_manager(cwd),
        support::model_registry().await,
        None,
    );
    runner
        .bind_core(support::actions(), support::context_actions(), None)
        .unwrap();
    runner
        .emit(
            &mut pi_coding_agent::core::extensions::types::ExtensionEvent::SessionShutdown(
                pi_coding_agent::core::extensions::types::SessionShutdownEvent {
                    reason: pi_coding_agent::core::extensions::types::SessionShutdownReason::Quit,
                    target_session_file: None,
                },
            ),
        )
        .await;
    let expected = oracle.request(&json!({"id":2,"operation":"disposal"}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(json!(events.lock().unwrap().clone()), expected["output"]);
}

#[tokio::test]
async fn pyrun_dispose_terminates_inherited_process_group_members_like_frozen() {
    use pi_coding_agent::bundled_extensions::pyrun::runner::{
        CanonicalPyrunEvalParams, PyrunRunnerClient, PyrunRunnerOptions,
    };
    use pi_coding_agent::core::detached_bash_runner::is_process_identity_alive;
    use pi_coding_agent::core::runtime_process::read_process_identity;
    use std::os::unix::process::CommandExt;
    let _environment = ENVIRONMENT.lock().await;
    process_env::set("PI_PYRUN_RUNNER_ARGS", "[]");
    let directory = tempfile::tempdir().unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-pyrun-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/runner.ts",
    );
    for frozen in [true, false] {
        let pid_path = directory.path().join("child-pid");
        let _ = std::fs::remove_file(&pid_path);
        let script = format!(
            "import subprocess,sys,time,pathlib\nchild=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)'])\npathlib.Path({:?}).write_text(str(child.pid))\ntime.sleep(30)",
            pid_path.to_str().unwrap()
        );
        let mut tree = std::process::Command::new("python3")
            .args(["-c", &script])
            .process_group(0)
            .spawn()
            .unwrap();
        for _ in 0..100 {
            if pid_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let child_pid = std::fs::read_to_string(&pid_path).unwrap().parse::<u32>().unwrap();
        let leader_identity = read_process_identity(tree.id()).unwrap();
        let child_identity = read_process_identity(child_pid).unwrap();
        let runner_script =
            "import sys,json\nfor line in sys.stdin:\n print(json.dumps({'type':'completed','value':42}),flush=True)";
        if frozen {
            let expected=oracle.request(&json!({"id":1,"cases":[{"params":{"code":"1+1"},"options":{"command":"python3","args":["-u","-c",runner_script],"detached":false,"inheritedProcessGroupId":tree.id()}}]}));
            assert_eq!(expected["ok"], json!(true), "{expected:?}");
        } else {
            let runner = PyrunRunnerClient::new(PyrunRunnerOptions {
                command: Some("python3".into()),
                args: Some(vec!["-u".into(), "-c".into(), runner_script.into()]),
                detached: Some(false),
                inherited_process_group_id: Some(tree.id()),
                ..Default::default()
            });
            let params: CanonicalPyrunEvalParams = json::from_value(&json!({"code":"1+1"})).unwrap();
            assert_eq!(
                runner.evaluate(params, None, None, None).await.unwrap().value,
                Some(json!(42))
            );
            runner.dispose();
        }
        for _ in 0..100 {
            if !is_process_identity_alive(&leader_identity) && !is_process_identity_alive(&child_identity) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let alive = is_process_identity_alive(&leader_identity) || is_process_identity_alive(&child_identity);
        let _ = rustix::process::kill_process_group(
            rustix::process::Pid::from_raw(tree.id() as i32).unwrap(),
            rustix::process::Signal::KILL,
        );
        tree.wait().unwrap();
        assert!(!alive, "inherited group descendants survived disposal, frozen={frozen}");
    }
    process_env::clear();
}

#[tokio::test]
async fn pyrun_progress_failure_rejects_only_its_queued_request_like_frozen() {
    use pi_coding_agent::bundled_extensions::pyrun::runner::{
        CanonicalPyrunEvalParams, PyrunRunnerClient, PyrunRunnerError, PyrunRunnerOptions,
    };
    let _environment = ENVIRONMENT.lock().await;
    process_env::set("PI_PYRUN_RUNNER_ARGS", "[]");
    let script = "import sys,json,time\nsys.stdin.readline()\nsys.stdin.readline()\nprint(json.dumps({'type':'console','text':'progress'}),flush=True)\nprint(json.dumps({'type':'completed','value':42}),flush=True)\ntime.sleep(0.2)";
    let mut oracle = oracle::TsOracle::spawn(
        "pyrun-runner-queue-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/runner.ts",
    );
    let expected = oracle.request(&json!({"id":1,"options":{"command":"python3","args":["-u","-c",script]}}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let runner = PyrunRunnerClient::new(PyrunRunnerOptions {
        command: Some("python3".into()),
        args: Some(vec!["-u".into(), "-c".into(), script.into()]),
        ..Default::default()
    });
    let first: CanonicalPyrunEvalParams = json::from_value(&json!({"code":"first"})).unwrap();
    let second: CanonicalPyrunEvalParams = json::from_value(&json!({"code":"second"})).unwrap();
    let (first, second) = tokio::join!(
        runner.evaluate(
            first,
            Some(std::sync::Arc::new(|_| Err(PyrunRunnerError::new("progress rejected")))),
            None,
            None
        ),
        runner.evaluate(second, None, None, None)
    );
    let actual = [first, second]
        .into_iter()
        .map(|result| match result {
            Ok(result) => json!({"result":result}),
            Err(error) => json!({"error":error.message()}),
        })
        .collect::<Vec<_>>();
    runner.dispose();
    assert_eq!(json!(actual), expected["output"]);
    process_env::clear();
}

#[tokio::test]
async fn pyrun_resume_resolves_metadata_name_in_the_context_control_db_like_frozen() {
    use pi_coding_agent::bundled_extensions::pyrun::{index::create_pyrun_pi_dispatcher, runner::PiRequest};
    use pi_coding_agent::core::{
        extensions::types::{ExtensionCommandContextActions, SessionTransitionResult},
        session_manager::SessionManager,
    };
    use std::sync::{Arc, Mutex};
    let _environment = ENVIRONMENT.lock().await;
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_str().unwrap();
    let path = directory.path().join("target.jsonl");
    let control = directory.path().join("control.sqlite");
    std::fs::write(&path,format!("{}\n{}\n",json::stringify_lossless(&json!({"type":"session","version":3,"id":"target-id","timestamp":"2026-01-01T00:00:00.000Z","cwd":cwd})),json::stringify_lossless(&json!({"type":"message","id":"user-1","parentId":null,"timestamp":"2026-01-01T00:00:00.000Z","message":{"role":"user","content":"hello","timestamp":0}})))).unwrap();
    let mut manager = SessionManager::open(&path.to_str().unwrap().into(), Some(&cwd.into()), None).unwrap();
    manager
        .set_metadata_control_db_path(Some(&control.to_str().unwrap().into()), Default::default())
        .unwrap();
    manager.set_session_name(Some(&"metadata-only".into())).unwrap();
    let runtime = support::runtime();
    let api = Arc::new(Mutex::new(None));
    let capture = api.clone();
    let extension = support::extension(&runtime, cwd, "resume", move |api| {
        *capture.lock().unwrap() = Some(api.clone())
    })
    .await;
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        cwd,
        Arc::new(Mutex::new(manager)),
        support::model_registry().await,
        None,
    );
    let mut context = support::context_actions();
    let control_path = control.to_str().unwrap().to_owned();
    context.get_control_db_path = Some(Arc::new(move || Some(control_path.clone().into())));
    runner.bind_core(support::actions(), context, None).unwrap();
    let selected = Arc::new(Mutex::new(None));
    let selection = selected.clone();
    runner.bind_command_context(Some(ExtensionCommandContextActions {
        show_approval_selector: Arc::new(|| Ok(())),
        show_sandbox_selector: Arc::new(|| Ok(())),
        wait_for_idle: Arc::new(|| Box::pin(async {})),
        new_session: Arc::new(|_| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        fork: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        navigate_tree: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        switch_session: Arc::new(move |path, _| {
            *selection.lock().unwrap() = Some(path);
            Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })
        }),
        reload: Arc::new(|| Box::pin(async { Ok(()) })),
        restart: Arc::new(|_| Box::pin(async { Ok(()) })),
    }));
    let dispatcher = create_pyrun_pi_dispatcher(&api.lock().unwrap().take().unwrap(), &Default::default());
    let actual = dispatcher(
        PiRequest {
            method: "sessions.resume".into(),
            params: json!({"name":"metadata-only"}),
        },
        runner.create_context(),
        None,
        None,
    )
    .await
    .unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "pyrun-registration-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/index.ts",
    );
    let expected =
        oracle.request(&json!({"id":1,"operation":"resume","cwd":cwd,"sessionDir":cwd,"controlDbPath":control}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        json!({"result":actual,"selected":selected.lock().unwrap().clone()}),
        expected["output"]
    );
}

#[test]
fn pyrun_empty_optional_text_renders_like_frozen() {
    use pi_coding_agent::{
        bundled_extensions::pyrun::index::create_pyrun_tool_definition,
        core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState},
        modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme},
        utils::ansi::strip_ansi,
    };
    let colors = REQUIRED_COLORS
        .iter()
        .map(|key| ((*key).into(), ColorValue::Text("#808080".into())))
        .collect::<Vec<_>>();
    let theme = Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap();
    let tool = create_pyrun_tool_definition(
        std::sync::Arc::new(|_, _, _, _, _| Box::pin(async { unreachable!() })),
        Default::default(),
    );
    let ctx = ToolRenderContext {
        args: json!({"code":"","session_id":""}),
        tool_call_id: "call".into(),
        invalidate: std::sync::Arc::new(|| {}),
        last_component: None,
        call_component: None,
        state: ToolRenderState::default(),
        cwd: "/tmp".into(),
        execution_started: false,
        args_complete: true,
        is_partial: false,
        expanded: false,
        show_images: false,
        is_error: false,
    };
    let call = tool.render_call.unwrap()(&ctx.args, &theme, &ctx)
        .unwrap()
        .borrow_mut()
        .render(80)
        .iter()
        .map(strip_ansi)
        .collect::<Vec<_>>();
    let result =
        json::from_value(&json!({"content":[{"type":"text","text":"\n\nvalue"}],"details":{"executed":""}})).unwrap();
    let output = tool.render_result.unwrap()(
        &result,
        &ToolRenderResultOptions {
            expanded: false,
            is_partial: false,
        },
        &theme,
        &ctx,
    )
    .unwrap()
    .borrow_mut()
    .render(80)
    .iter()
    .map(strip_ansi)
    .collect::<Vec<_>>();
    let mut oracle = oracle::TsOracle::spawn(
        "pyrun-registration-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/index.ts",
    );
    let expected = oracle.request(&json!({"id":1,"operation":"render-empty"}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(json!({"call":call,"result":output}), expected["output"]);
}
