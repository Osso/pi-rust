#[test]
fn startup_resources_provider_count_scope_and_restored_frames_match_frozen() {
    mod oracle {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
    }
    let mut source = oracle::TsOracle::spawn("interactive-startup-oracle.mts", "packages/coding-agent/src");
    for (quiet, verbose, scoped, restored) in [
        (false, false, false, false),
        (true, false, false, false),
        (true, false, true, false),
        (true, true, true, false),
        (false, false, true, false),
        (false, false, false, true),
        (false, false, true, true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("project");
        std::fs::create_dir_all(&cwd).unwrap();
        let skill = root.path().join("discovered");
        std::fs::create_dir_all(&skill).unwrap();
        let skill = skill.join("SKILL.md");
        std::fs::write(
            &skill,
            "---\nname: post-start\ndescription: A startup-discovered skill\n---\nSkill body\n",
        )
        .unwrap();
        let watch_changes = !quiet && !verbose && !scoped && !restored;
        if watch_changes {
            let status = std::process::Command::new("git")
                .args(["init", "--initial-branch=before-startup"])
                .arg(&cwd)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
        }
        let request = serde_json::json!({"cwd":cwd,"agentDir":cwd,"skillPath":skill,"quiet":quiet,"verbose":verbose,"scoped":scoped,"restored":restored,"watchChanges":watch_changes});
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "modes::interactive::interactive_mode::ui_tests::startup_capture_child",
                "--nocapture",
            ])
            .env("PI_INTERACTIVE_STARTUP_CASE", request.to_string())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child failed: {}\n{}",
            String::from_utf8(output.stdout.clone()).unwrap(),
            String::from_utf8(output.stderr).unwrap()
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let native_line = stdout
            .lines()
            .find_map(|line| line.strip_prefix("STARTUP_RESULT "))
            .expect("startup snapshot");
        let mut native: serde_json::Value = serde_json::from_str(native_line).unwrap();
        let console = stdout
            .split("STARTUP_BEGIN\n")
            .nth(1)
            .unwrap()
            .split("STARTUP_END\n")
            .next()
            .unwrap();
        native["output"]["console"] = serde_json::json!(
            console
                .lines()
                .map(|line| JsString::from(line).units().to_vec())
                .collect::<Vec<_>>()
        );
        if watch_changes {
            std::fs::write(cwd.join(".git/HEAD"), "ref: refs/heads/before-startup\n").unwrap();
        }
        let expected = source.request(&native["request"]);
        assert_eq!(expected["ok"], true, "source: {expected}");
        assert_eq!(
            JsValue::from(&native["output"]),
            JsValue::from(&expected["output"]),
            "quiet={quiet},verbose={verbose},scoped={scoped},restored={restored}"
        );
    }
}

#[test]
fn import_error_class_controls_recovery_and_exit_like_frozen() {
    assert_import_cases_match_frozen(&["missing", "factory"]);
}

#[test]
fn import_command_dialogs_state_and_retry_match_frozen() {
    assert_import_cases_match_frozen(&["cancel", "hook-cancel", "cwd-cancel", "cwd-retry-error", "success"]);
}

fn assert_import_cases_match_frozen(cases: &[&str]) {
    mod oracle {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
    }
    for failure in cases.iter().copied() {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("project");
        std::fs::create_dir_all(&cwd).unwrap();
        let skill = root.path().join("SKILL.md");
        std::fs::write(
            &skill,
            "---\nname: import-probe\ndescription: Import fixture\n---\nbody\n",
        )
        .unwrap();
        let path = root.path().join("imported session.jsonl");
        if failure != "missing" {
            let stored_cwd = if failure.starts_with("cwd-") {
                cwd.join("deleted-project")
            } else {
                cwd.clone()
            };
            let header = serde_json::json!({"type":"session","version":3,"id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","timestamp":"1970-01-01T00:00:00.000Z","cwd":stored_cwd});
            let message = serde_json::json!({"type":"message","id":"abc12345","parentId":null,"timestamp":"1970-01-01T00:00:00.000Z","message":{"role":"user","content":"Imported content","timestamp":0}});
            std::fs::write(&path, format!("{header}\n{message}\n")).unwrap();
        }
        let request = serde_json::json!({"cwd":cwd,"agentDir":cwd,"skillPath":skill,"quiet":true,"verbose":false,"scoped":false,"restored":false,"watchChanges":false,"importFailure":failure,"importPath":path});
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "modes::interactive::interactive_mode::ui_tests::startup_capture_child",
                "--nocapture",
            ])
            .env("PI_INTERACTIVE_STARTUP_CASE", request.to_string())
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let request = stdout
            .lines()
            .find_map(|line| line.strip_prefix("IMPORT_REQUEST "))
            .unwrap_or_else(|| panic!("no request: {stdout}\n{}", String::from_utf8(output.stderr).unwrap()));
        let request: serde_json::Value = serde_json::from_str(request).unwrap();
        let result = stdout
            .lines()
            .find_map(|line| line.strip_prefix("IMPORT_RESULT "))
            .unwrap_or_else(|| panic!("no snapshot: {stdout}"));
        let mut actual: serde_json::Value = serde_json::from_str(result).unwrap();
        actual["exitCode"] = serde_json::json!(output.status.code().expect("normal process exit"));
        let mut source = oracle::TsOracle::spawn("interactive-startup-oracle.mts", "packages/coding-agent/src");
        let expected = source.request(&request);
        assert_eq!(expected["ok"], true, "{expected}");
        assert_eq!(
            JsValue::from(actual),
            JsValue::from(&expected["output"]),
            "failure={failure}"
        );
    }
}

async fn drive_fixture_import(
    mode: &Rc<InteractiveMode>,
    terminal: &Rc<RefCell<CaptureTerminal>>,
    request: &serde_json::Value,
    factory_failure: &Arc<Mutex<Option<CallbackError>>>,
) {
    println!("IMPORT_REQUEST {request}");
    let case = request["importFailure"].as_str().unwrap().to_owned();
    let dialogs = Rc::new(RefCell::new(Vec::<Vec<Vec<u16>>>::new()));
    let rejected = Rc::new(RefCell::new(None::<JsValue>));
    let capture_dialogs = dialogs.clone();
    let capture_rejection = rejected.clone();
    let observer = Rc::downgrade(mode);
    let path = request["importPath"].as_str().unwrap().to_owned();
    let success = case == "success";
    terminal.borrow_mut().lifecycle_observer = Some(Rc::new(move |event| {
        if event == "stop" {
            let mode = observer.upgrade().unwrap();
            let render = |view: &mut dyn Component| {
                view.render(120)
                    .into_iter()
                    .map(|line| line.units().to_vec())
                    .collect::<Vec<_>>()
            };
            let mut output = serde_json::json!({"chat":render(&mut *mode.chat_container.borrow_mut()),"status":render(&mut *mode.status_container.borrow_mut()),"dialogs":*capture_dialogs.borrow(),"rejected":*capture_rejection.borrow()});
            if success {
                output["messages"] = serde_json::to_value(mode.session().messages()).unwrap();
                output["copiedExact"] = serde_json::json!(
                    std::fs::read(encode_node_utf8(&mode.session().session_file().unwrap())).unwrap()
                        == std::fs::read(&path).unwrap()
                );
            }
            println!("IMPORT_RESULT {output}");
        }
        Ok(())
    }));
    mode.status_container
        .borrow_mut()
        .add_child(component(Text::new("pre-import status", 0, 0)));
    if case == "factory" || case == "cwd-retry-error" {
        *factory_failure.lock().unwrap() = Some(CallbackError::new("File not found: imported dependency"));
    }
    let command = js_format!("/import '{}' ignored", request["importPath"].as_str().unwrap());
    let actor = mode.clone();
    let operation = tokio::task::spawn_local(async move { actor.handle_import_command(command).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut last_dialog = Vec::<JsString>::new();
        while !operation.is_finished() {
            mode.handle_ui_requests().await;
            let lines = mode.editor_container.borrow_mut().render(120);
            let frame = JsString::join(&lines, "\n");
            let is_dialog = frame.contains("Import session") || frame.contains("Session cwd not found");
            if is_dialog && lines != last_dialog {
                last_dialog = lines.clone();
                dialogs
                    .borrow_mut()
                    .push(lines.into_iter().map(|line| line.units().to_vec()).collect());
                let cancel = case == "cancel" || (case == "cwd-cancel" && dialogs.borrow().len() == 2);
                terminal.borrow_mut().send_input(if cancel { "\u{1b}" } else { "\r" });
                mode.ui.process_events();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("import UI operation settles");
    if let Err(error) = operation.await.unwrap() {
        *rejected.borrow_mut() = Some(error.thrown_value().as_error().unwrap().value());
    }
    mode.stop().unwrap();
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "invoked by startup/import process-boundary parents"]
async fn startup_capture_child() {
    use crate::core::extensions::types::{
        HandlerHandle, ResourcesDiscoverEvent, ResourcesDiscoverResult, SessionBeforeSwitchEvent,
        SessionBeforeSwitchResult,
    };
    use crate::core::resource_loader::{
        DefaultResourceLoader, DefaultResourceLoaderOptions, InlineExtension, ResourceLoader,
    };
    tokio::task::LocalSet::new().run_until(async {
        let mut request: serde_json::Value = serde_json::from_str(&std::env::var("PI_INTERACTIVE_STARTUP_CASE").unwrap()).unwrap();
        let cwd = request["cwd"].as_str().unwrap();
        let path = JsString::from(request["skillPath"].as_str().unwrap());
        let cancel_switch = request["importFailure"].as_str() == Some("hook-cancel");
        let factory: crate::core::extensions::ExtensionFactory = Arc::new(move |api| {
            let path = path.clone();
            let handler: HandlerHandle<ResourcesDiscoverEvent, ResourcesDiscoverResult> = Arc::new(move |_, _| -> BoxFuture<'static, Result<Option<ResourcesDiscoverResult>, HandlerError>> {
                let path = path.clone();
                Box::pin(async move { Ok(Some(ResourcesDiscoverResult { skill_paths: Some(vec![path]), prompt_paths: None, theme_paths: None })) })
            });
            api.on_resources_discover(handler).unwrap();
            if cancel_switch {
                let handler: HandlerHandle<SessionBeforeSwitchEvent, SessionBeforeSwitchResult> = Arc::new(|_, _| -> BoxFuture<'static, Result<Option<SessionBeforeSwitchResult>, HandlerError>> { Box::pin(async { Ok(Some(SessionBeforeSwitchResult { cancel: Some(true) })) }) });
                api.on_session_before_switch(handler).unwrap();
            }
            Box::pin(async { Ok(()) })
        });
        let settings = SettingsManager::in_memory(pi_js_compat::js_json!({"quietStartup":request["quiet"].as_bool().unwrap()}).as_object().unwrap().clone(), SettingsManagerCreateOptions::default());
        let loader = Arc::new(DefaultResourceLoader::new(DefaultResourceLoaderOptions {
            cwd: cwd.into(), agent_dir: cwd.into(), settings_manager: Some(Arc::new(Mutex::new(SettingsManager::in_memory(settings.get_global_settings(), SettingsManagerCreateOptions::default())))),
            extension_factories: vec![InlineExtension::Factory { extension_path: None, factory }],
            no_extensions: true, no_skills: true, no_prompt_templates: true, no_themes: true, no_context_files: true,
            ..Default::default()
        }).unwrap());
        loader.reload().await.unwrap();
        let factory_failure = Arc::new(Mutex::new(None));
        let (initial, terminal, _faux, _temp) = build_mode_with_extra_providers(loader.clone(), None, settings, Vec::new(), serde_json::Map::new(), request["importFailure"].is_string(), TestModeSetup { cwd: Some(cwd.into()), runtime_factory_failure: Some(factory_failure.clone()), ..Default::default() }).await;
        let models: Vec<_> = initial.session().model_registry().get_all().into_iter().filter(|model| model.provider == "faux").collect();
        request["models"] = serde_json::to_value(&models).unwrap();
        request["colors"] = serde_json::json!(crate::modes::interactive::theme::REQUIRED_COLORS);
        if request["scoped"].as_bool().unwrap() {
            initial.session().set_scoped_models(models.into_iter().enumerate().map(|(index, model)| crate::core::model_resolver::ScopedModel { model, thinking_level: Some(if index == 0 { ThinkingLevel::Off } else { ThinkingLevel::Medium }) }).collect());
        }
        if request["restored"].as_bool().unwrap() {
            let message: AgentMessage = js_json::from_value(&pi_js_compat::js_json!({"role":"user","content":"Restored startup message","timestamp":0})).unwrap();
            initial.session().session_manager().append_message(message.clone()).unwrap();
            initial.session().agent().set_messages(vec![message]);
        }
        terminal.borrow_mut().columns = 120;
        terminal.borrow_mut().rows = 40;
        let terminal_trait: Rc<RefCell<dyn Terminal>> = terminal.clone();
        let mode = InteractiveMode::new(initial.runtime.clone(), InteractiveModeOptions { verbose: request["verbose"].as_bool().unwrap(), ..Default::default() }, Some(terminal_trait)).await.unwrap();
        println!("\nSTARTUP_BEGIN");
        mode.init().await.unwrap();
        println!("STARTUP_END");
        if request["importFailure"].is_string() {
            drive_fixture_import(&mode, &terminal, &request, &factory_failure).await;
            return;
        }
        let render = |container: &mut dyn Component| container.render(120).into_iter().map(|line| line.units().to_vec()).collect::<Vec<_>>();
        let mut output = serde_json::json!({
            "header":render(&mut *mode.header_container.borrow_mut()),"resources":render(&mut *mode.loaded_resources_container.borrow_mut()),"chat":render(&mut *mode.chat_container.borrow_mut()),
            "count":mode.footer_data_provider.get_available_provider_count(),"titles":terminal.borrow().titles.iter().map(|title|title.units().to_vec()).collect::<Vec<_>>(),"console":[],
            "skills":loader.get_skills().skills.iter().map(|skill|skill.name.clone()).collect::<Vec<_>>()
        });
        if request["watchChanges"].as_bool().unwrap() {
            mode.ui.flush_pending().unwrap();
            terminal.borrow_mut().writes.clear();
            let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS.iter().map(|name|((*name).into(),crate::modes::interactive::theme::ColorValue::Index(0.0))).collect();
            crate::modes::interactive::theme::set_theme_instance(Arc::new(Theme::new(&colors,&colors,crate::modes::interactive::theme::ColorMode::Color256,None,None).unwrap()));
            mode.handle_ui_requests().await;
            mode.ui.flush_pending().unwrap();
            let theme_repaint = !terminal.borrow().writes.is_empty();
            let editor = mode.editor().borrow_mut().render(120).into_iter().map(|line|line.units().to_vec()).collect::<Vec<_>>();
            terminal.borrow_mut().writes.clear();
            std::fs::write(Path::new(request["cwd"].as_str().unwrap()).join(".git/HEAD"), "ref: refs/heads/after-startup\n").unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            while !terminal.borrow().output().contains("after-startup") && Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(10)).await;
                mode.handle_ui_requests().await;
                mode.ui.flush_pending().unwrap();
            }
            output["changes"] = serde_json::json!({"themeRepaint":theme_repaint,"editor":editor,"branchRepaint":terminal.borrow().output().contains("after-startup")});
        }
        println!("STARTUP_RESULT {}", serde_json::json!({"request":request,"output":output}));
        mode.stop().unwrap();
    }).await;
}
