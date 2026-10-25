#[test]
fn help_commands_match_frozen_complete_frames() {
    assert_help_cases(&["hotkeys", "hotkeys-custom", "changelog", "changelog-empty"]);
}
#[test]
fn changelog_selection_and_startup_notices_match_frozen() {
    assert_help_cases(&[
        "fresh",
        "empty-version",
        "upgraded",
        "current",
        "resumed",
        "notice-full",
        "notice-collapse",
        "notice-fallback",
        "notice-empty",
    ]);
}
#[test]
fn install_telemetry_guards_and_private_fetch_match_frozen() {
    assert_help_cases(&[
        "telemetry-offline",
        "telemetry-empty",
        "telemetry-disabled",
        "telemetry-failure",
    ]);
}

fn assert_help_cases(cases: &[&str]) {
    mod oracle {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
    }
    let root = tempfile::tempdir().unwrap();
    write_help_package(root.path());
    let env = [
        ("PI_PACKAGE_DIR", root.path().as_os_str().to_owned()),
        ("PI_CODING_AGENT_DIR", root.path().as_os_str().to_owned()),
    ];
    let mut source = oracle::TsOracle::spawn_with_environment(
        "interactive-help-notices-oracle.mts",
        "packages/coding-agent/src",
        &env,
    );
    for case in cases {
        let changelog = if *case == "changelog-empty" {
            ""
        } else {
            "## [9.8.7]\n- Latest [guide](./docs/latest.md)\n\n## [1.2.3]\n- Middle\n\n## [1.0.0]\n- Old\n"
        };
        std::fs::write(root.path().join("CHANGELOG.md"), changelog).unwrap();
        let bindings = if *case == "hotkeys-custom" {
            serde_json::json!({"app.clear":["alt+x","ctrl+shift+z"],"tui.editor.cursorUp":[],"app.model.cycleForward":"f6"})
        } else {
            serde_json::json!({})
        };
        std::fs::write(root.path().join("keybindings.json"), bindings.to_string()).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "modes::interactive::interactive_mode::ui_tests::help_notices_child",
                "--nocapture",
            ])
            .envs(env.iter().map(|(key, value)| (*key, value)))
            .env("PI_HELP_CASE", case)
            .env(
                "PI_OFFLINE",
                if *case == "telemetry-offline" || !case.starts_with("telemetry") {
                    "1"
                } else {
                    ""
                },
            )
            .env("PI_TELEMETRY", if *case == "telemetry-disabled" { "0" } else { "1" })
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{case}: {}\n{}",
            String::from_utf8(output.stdout.clone()).unwrap(),
            String::from_utf8(output.stderr).unwrap()
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let native = pi_js_compat::js_json::parse_lossless(
            stdout
                .lines()
                .find_map(|line| line.strip_prefix("HELP_RESULT "))
                .unwrap(),
        )
        .unwrap();
        let expected = source.request(native.get("request").unwrap());
        assert_eq!(expected.get("ok"), Some(&JsValue::Bool(true)), "{case}: {expected:?}");
        let expected = expected_with_native_runtime(expected.get("output").unwrap().clone());
        assert_eq!(native.get("output"), Some(&expected), "{case}");
    }
}

// Approved pi-user-agent runtime-token gap; every other request/header byte remains compared.
fn expected_with_native_runtime(mut output: JsValue) -> JsValue {
    let requests = output
        .get("requests")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|request| {
            let headers = request
                .get("headers")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(native_runtime_header)
                .collect::<Vec<_>>();
            let mut request = request.clone();
            request
                .as_object_mut()
                .unwrap()
                .insert("headers".into(), JsValue::Array(headers));
            request
        })
        .collect::<Vec<_>>();
    output
        .as_object_mut()
        .unwrap()
        .insert("requests".into(), JsValue::Array(requests));
    output
}

fn native_runtime_header(header: &JsValue) -> JsValue {
    let cells = header.as_array().unwrap();
    if cells[0].as_string().unwrap() != "User-Agent" {
        return header.clone();
    }
    let value = cells[1].as_string().unwrap().to_utf8().unwrap();
    let parts = value.split(';').collect::<Vec<_>>();
    assert_eq!(parts.len(), 3);
    assert!(parts[1].starts_with(" node/") || parts[1].starts_with(" bun/"));
    let adjusted = format!("{}; rust;{}", parts[0], parts[2]);
    pi_js_compat::js_json!(["User-Agent", adjusted])
}

fn write_help_package(root: &Path) {
    let package = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"));
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(package.join("package.json")).unwrap()).unwrap();
    manifest["version"] = serde_json::json!("9.8.7");
    std::fs::write(root.join("package.json"), manifest.to_string()).unwrap();
    for dir in ["theme", "src/modes/interactive/theme"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
        for name in ["dark.json", "light.json"] {
            std::fs::copy(package.join("theme").join(name), root.join(dir).join(name)).unwrap();
        }
    }
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "invoked by help/notices/telemetry process parents"]
async fn help_notices_child() {
    tokio::task::LocalSet::new().run_until(async {
        let case = std::env::var("PI_HELP_CASE").unwrap();
        let root = std::env::var("PI_PACKAGE_DIR").unwrap();
        let mut settings = pi_js_compat::js_json!({"collapseChangelog":case == "notice-collapse" || case == "notice-fallback","codeBlockIndent":"  "});
        if case == "empty-version" { settings.as_object_mut().unwrap().insert("lastChangelogVersion".into(), JsValue::String("".into())); }
        if case == "upgraded" || case == "resumed" { settings.as_object_mut().unwrap().insert("lastChangelogVersion".into(), JsValue::String("1.0.0".into())); }
        if case == "current" { settings.as_object_mut().unwrap().insert("lastChangelogVersion".into(), JsValue::String("9.8.7".into())); }
        let runtime = crate::core::extensions::create_extension_runtime();
        let bus: Arc<dyn crate::core::event_bus::EventBus> = Arc::new(crate::core::event_bus::EventBusController::new());
        let factory: crate::core::extensions::ExtensionFactory = Arc::new(|api| {
            for (key, description) in [("alt+shift+q",Some("説明".into())),("alt+shift+w",None),("alt+shift+e",Some("".into()))] {
                api.register_shortcut(key.into(), description, Arc::new(|_| Box::pin(async { Ok(()) }))).unwrap();
            }
            Box::pin(async { Ok(()) })
        });
        let extensions = if case.starts_with("hotkeys") { vec![crate::core::extensions::load_extension_from_factory(factory, &root.clone().into(), bus, runtime.clone(), &"/fixture/hotkeys.rs".into()).await.unwrap()] } else { Vec::new() };
        let loader = Arc::new(StaticResourceLoader { extensions: Some(crate::core::extensions::types::LoadExtensionsResult { extensions, errors: Vec::new(), runtime }), ..Default::default() });
        let (mode, _terminal, _faux, _temp) = build_mode_with_extra_providers(loader, None,
            SettingsManager::in_memory(settings.as_object().unwrap().clone(), SettingsManagerCreateOptions::default()),
            Vec::new(), serde_json::Map::new(), false, TestModeSetup { cwd: Some(root.clone()), ..Default::default() },
        ).await;
        if case == "resumed" { mode.session().agent().set_messages(vec![AgentMessage::User(pi_ai::types::UserMessage { role: Default::default(), content: pi_ai::types::UserContent::Text("Resumed".into()), input_source: None, provider_native: None, timestamp: 0.0 })]); }
        let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS.iter().map(|name| ((*name).into(),crate::modes::interactive::theme::ColorValue::Index(0.0))).collect();
        crate::modes::interactive::theme::set_theme_instance(Arc::new(Theme::new(&colors,&colors,crate::modes::interactive::theme::ColorMode::Color256,None,None).unwrap()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        *mode.http_fetch.borrow_mut() = Some(Arc::new(HelpRecordingFetch { requests: requests.clone(), fail: case == "telemetry-failure" }));
        let markdown = match case.as_str() { "notice-empty" => Some(JsString::default()), "notice-fallback" => Some("No version header\n- Fallback".into()), value if value.starts_with("notice") => Some("  ## [9.8.7]\n- Notice\n\n```txt\ncode\n```\n  ".into()), _ => None };
        *mode.changelog_markdown.borrow_mut() = markdown.clone();
        let existing = case == "notice-full";
        if existing { mode.chat_container.borrow_mut().add_child(component(Text::new("Existing chat", 0, 0))); }
        let shortcuts = mode.session().extension_runner().get_shortcuts(&mode.keybindings.get_effective_config()).into_iter().map(|(key,shortcut)| pi_js_compat::js_json!({"key":key,"description":shortcut.description,"extensionPath":shortcut.extension_path})).collect::<Vec<_>>();
        let request = pi_js_compat::js_json!({"case":case,"root":root,"settings":settings,"markdown":markdown,"resumed":case == "resumed","existing":existing,"offline":std::env::var("PI_OFFLINE").ok(),"telemetry":std::env::var("PI_TELEMETRY").unwrap(),"colors":crate::modes::interactive::theme::REQUIRED_COLORS,"shortcuts":shortcuts});
        let mut frames = Vec::new();
        let display = if case.starts_with("hotkeys") { mode.handle_hotkeys_command(); None }
            else if case.starts_with("changelog") { mode.handle_changelog_command().unwrap(); None }
            else if case.starts_with("notice") { mode.show_startup_notices_if_needed(); frames.push(help_frames(&mode)); mode.show_startup_notices_if_needed(); None }
            else if case.starts_with("telemetry") { mode.report_install_telemetry(&"9.8.7+build".into()); None }
            else { mode.get_changelog_for_display().unwrap() };
        for _ in 0..3 { tokio::task::yield_now().await; }
        frames.push(help_frames(&mode));
        let requests = requests.lock().unwrap().iter().map(|request: &HttpRequest| pi_js_compat::js_json!({"url":request.url,"method":request.method,"headers":request.headers})).collect::<Vec<_>>();
        let output = pi_js_compat::js_json!({"frames":frames,"markdown":display,"seen":mode.settings(|settings| settings.get_last_changelog_version().cloned()),"requests":requests,"shown":mode.startup_notices_shown.get()});
        println!("HELP_RESULT {}", pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json!({"request":request,"output":output})));
        mode.stop().unwrap();
    }).await;
}

fn help_frames(mode: &InteractiveMode) -> Vec<Vec<Vec<u16>>> {
    [25, 120]
        .into_iter()
        .map(|width| {
            mode.chat_container
                .borrow_mut()
                .render(width)
                .into_iter()
                .map(|line| line.units().to_vec())
                .collect()
        })
        .collect()
}
struct HelpRecordingFetch {
    requests: Arc<Mutex<Vec<HttpRequest>>>,
    fail: bool,
}
impl Fetch for HelpRecordingFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request);
        Box::pin(async move {
            if self.fail {
                Err(CallbackError::new("fixture network rejection"))
            } else {
                Ok(pi_ai::request::HttpResponse {
                    status: 200,
                    status_text: "OK".into(),
                    headers: Vec::new(),
                    body: Box::new(EmptyBody),
                })
            }
        })
    }
}
