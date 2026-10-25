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
    mod oracle {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
    }
    for failure in ["missing", "factory"] {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("project");
        std::fs::create_dir_all(&cwd).unwrap();
        let skill = root.path().join("SKILL.md");
        std::fs::write(
            &skill,
            "---\nname: import-probe\ndescription: Import fixture\n---\nbody\n",
        )
        .unwrap();
        let path = root.path().join("imported.jsonl");
        if failure == "factory" {
            std::fs::write(&path, format!("{}\n", serde_json::json!({"type":"session","version":3,"id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","timestamp":"1970-01-01T00:00:00.000Z","cwd":cwd}))).unwrap();
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

#[tokio::test(flavor = "current_thread")]
#[ignore = "invoked by startup/import process-boundary parents"]
async fn startup_capture_child() {
    use crate::core::extensions::types::{HandlerHandle, ResourcesDiscoverEvent, ResourcesDiscoverResult};
    use crate::core::resource_loader::{
        DefaultResourceLoader, DefaultResourceLoaderOptions, InlineExtension, ResourceLoader,
    };
    tokio::task::LocalSet::new().run_until(async {
        let mut request: serde_json::Value = serde_json::from_str(&std::env::var("PI_INTERACTIVE_STARTUP_CASE").unwrap()).unwrap();
        let cwd = request["cwd"].as_str().unwrap();
        let path = JsString::from(request["skillPath"].as_str().unwrap());
        let factory: crate::core::extensions::ExtensionFactory = Arc::new(move |api| {
            let path = path.clone();
            let handler: HandlerHandle<ResourcesDiscoverEvent, ResourcesDiscoverResult> = Arc::new(move |_, _| -> BoxFuture<'static, Result<Option<ResourcesDiscoverResult>, HandlerError>> {
                let path = path.clone();
                Box::pin(async move { Ok(Some(ResourcesDiscoverResult { skill_paths: Some(vec![path]), prompt_paths: None, theme_paths: None })) })
            });
            api.on_resources_discover(handler).unwrap();
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
        if let Some(failure) = request["importFailure"].as_str() {
            println!("IMPORT_REQUEST {request}");
            let observer = Rc::downgrade(&mode);
            terminal.borrow_mut().lifecycle_observer = Some(Rc::new(move |event| {
                if event == "stop" {
                    let mode = observer.upgrade().unwrap();
                    let chat = mode.chat_container.borrow_mut().render(120).into_iter().map(|line|line.units().to_vec()).collect::<Vec<_>>();
                    println!("IMPORT_RESULT {}",serde_json::json!({"chat":chat}));
                }
                Ok(())
            }));
            if failure == "factory" {
                *factory_failure.lock().unwrap() = Some(CallbackError::new("File not found: imported dependency"));
            }
            let command = js_format!("/import {}",request["importPath"].as_str().unwrap());
            let actor = mode.clone();
            let operation = tokio::task::spawn_local(async move { actor.handle_import_command(command).await });
            tokio::time::timeout(Duration::from_secs(5), async {
                let mut confirmed = false;
                while !operation.is_finished() {
                    mode.handle_ui_requests().await;
                    let frame = JsString::join(mode.editor_container.borrow_mut().render(120), "\n");
                    if !confirmed && frame.contains("Import session") {
                        terminal.borrow_mut().send_input("\r");
                        mode.ui.process_events();
                        confirmed = true;
                    }
                    tokio::task::yield_now().await;
                }
            }).await.expect("import UI operation settles");
            operation.await.unwrap();
            mode.stop().unwrap();
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
