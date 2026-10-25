#[test]
fn name_commands_match_frozen_frames_persistence_and_events() {
    assert_display_commands("names");
}
#[test]
fn inventory_commands_match_frozen_frames_and_repeated_updates() {
    assert_display_commands("inventories");
    assert_display_commands("empty-inventories");
}

fn assert_display_commands(case: &str) {
    mod oracle {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "modes::interactive::interactive_mode::ui_tests::display_commands_child",
            "--nocapture",
        ])
        .env("PI_DISPLAY_CASE", case)
        .env("PI_DISPLAY_ROOT", root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8(output.stdout.clone()).unwrap(),
        String::from_utf8(output.stderr).unwrap()
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let line = stdout
        .lines()
        .find_map(|line| line.strip_prefix("DISPLAY_RESULT "))
        .unwrap();
    let native = pi_js_compat::js_json::parse_lossless(line).unwrap();
    let mut source = oracle::TsOracle::spawn("interactive-display-commands-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(native.get("request").unwrap());
    assert_eq!(expected.get("ok"), Some(&JsValue::Bool(true)), "{expected:?}");
    let expected = expected.get("snapshots").unwrap().as_array().unwrap();
    let actual = native.get("snapshots").unwrap().as_array().unwrap();
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual, expected, "{case} step {index}");
    }
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "invoked by name/inventory command parents"]
async fn display_commands_child() {
    tokio::task::LocalSet::new().run_until(async {
        let case = std::env::var("PI_DISPLAY_CASE").unwrap();
        let cwd = std::env::var("PI_DISPLAY_ROOT").unwrap();
        let extension_runtime = crate::core::extensions::create_extension_runtime();
        let event_bus: Arc<dyn crate::core::event_bus::EventBus> = Arc::new(crate::core::event_bus::EventBusController::new());
        let mut extensions = Vec::new();
        if case == "inventories" {
            for path in ["/fixture/日本/zebra.rs", "<first-party:inventory>"] {
                let factory: crate::core::extensions::ExtensionFactory = Arc::new(|_| Box::pin(async { Ok(()) }));
                extensions.push(crate::core::extensions::load_extension_from_factory(factory, &cwd.clone().into(), event_bus.clone(), extension_runtime.clone(), &path.into()).await.unwrap());
            }
        }
        let loader = Arc::new(StaticResourceLoader { extensions: Some(crate::core::extensions::types::LoadExtensionsResult { extensions, errors: Vec::new(), runtime: extension_runtime }), ..Default::default() });
        let (mode, terminal, _faux, _temp) = build_mode_with_extra_providers(loader, None,
            SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default()),
            Vec::new(), serde_json::Map::new(), true, TestModeSetup { cwd: Some(cwd), ..Default::default() },
        ).await;
        mode.session().session_manager().persist_for_recovery().unwrap();
        let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS.iter().map(|name| ((*name).into(), crate::modes::interactive::theme::ColorValue::Index(0.0))).collect();
        crate::modes::interactive::theme::set_theme_instance(Arc::new(Theme::new(&colors, &colors, crate::modes::interactive::theme::ColorMode::Color256, None, None).unwrap()));
        let events = Arc::new(Mutex::new(Vec::<JsValue>::new()));
        let observed = events.clone();
        let subscription = mode.session().subscribe(Arc::new(move |event| {
            if let AgentSessionEvent::SessionInfoChanged(event) = event { observed.lock().unwrap().push(event.name.clone().map(JsValue::String).unwrap_or(JsValue::Null)); }
        }));
        let tools = pi_js_compat::js_json::to_value(&mode.session().get_all_tools()).unwrap();
        let extension_values = mode.session().resource_loader().get_extensions().extensions.iter().map(|extension| {
            let extension = extension.read();
            pi_js_compat::js_json!({"path":extension.path,"sourceInfo":extension.source_info,"commands":extension.commands.len(),"tools":extension.tools.len(),"handlers":extension.handlers.len()})
        }).collect::<Vec<_>>();
        let commands: Vec<(&str, &str)> = if case == "names" {
            vec![("name", ""), ("name", "  First 日本  "), ("name", ""), ("name", "one\n two"), ("name", "two\0parts"), ("unname", ""), ("name", ""), ("unname", "")]
        } else { vec![("tools", ""), ("tools", ""), ("extensions", ""), ("extensions", "")] };
        let mut steps = Vec::new();
        let mut snapshots = Vec::new();
        for (index, (command, args)) in commands.iter().enumerate() {
            if case == "inventories" && index == 1 { mode.session().set_active_tools_by_name(&[]).unwrap(); }
            steps.push(pi_js_compat::js_json!({"command":command,"args":args,"active":mode.session().get_active_tool_names()}));
            assert!(mode.handle_slash_command(&(*command).into(), &(*args).into()).await.unwrap());
            let session = mode.session();
            let manager = session.session_manager();
            let mut reopened = SessionManager::open(manager.get_session_file().unwrap(), None, None).unwrap();
            reopened.set_metadata_control_db_path(manager.get_metadata_control_db_path(), Default::default()).unwrap();
            let frames = [25,120].into_iter().map(|width| mode.chat_container.borrow_mut().render(width).into_iter().map(|line| line.units().to_vec()).collect::<Vec<_>>()).collect::<Vec<_>>();
            snapshots.push(pi_js_compat::js_json!({
                "frames":frames,
                "name":manager.get_session_name(),"persistedName":reopened.get_session_name(),"events":events.lock().unwrap().clone(),
                "titles":terminal.borrow().titles.iter().map(|title| title.units().to_vec()).collect::<Vec<_>>()
            }));
        }
        let session = mode.session();
        let manager = session.session_manager();
        let request = pi_js_compat::js_json!({"sessionPath":manager.get_session_file(),"controlDb":manager.get_metadata_control_db_path(),"colors":crate::modes::interactive::theme::REQUIRED_COLORS,"tools":tools,"extensions":extension_values,"steps":steps});
        println!("DISPLAY_RESULT {}", pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json!({"request":request,"snapshots":snapshots})));
        drop(manager);
        session.unsubscribe(subscription);
        mode.stop().unwrap();
    }).await;
}
