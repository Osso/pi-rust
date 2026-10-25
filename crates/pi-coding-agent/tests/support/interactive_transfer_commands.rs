#[test]
fn export_commands_match_frozen_frames_and_private_effects() {
    assert_transfer_cases(&["export-jsonl", "export-html"]);
}

#[test]
fn copy_commands_match_frozen_frames_and_private_effects() {
    assert_transfer_cases(&["copy-none", "copy-text", "copy-error"]);
}

#[test]
fn share_commands_match_frozen_frames_and_private_effects() {
    assert_transfer_cases(&[
        "share-missing",
        "share-auth",
        "share-success",
        "share-error",
        "share-cancel",
    ]);
}

fn assert_transfer_cases(cases: &[&str]) {
    mod oracle {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
    }
    let mut source = oracle::TsOracle::spawn("interactive-transfer-oracle.mts", "packages/coding-agent/src");
    for case in cases {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "modes::interactive::interactive_mode::ui_tests::transfer_command_child",
                "--nocapture",
            ])
            .env("PI_TRANSFER_CASE", case)
            .env("PI_TRANSFER_ROOT", root.path())
            .env("PATH", root.path().join("bin"))
            .env("TMPDIR", root.path().join("tmp"))
            .env("WAYLAND_DISPLAY", "fixture-wayland")
            .env("XDG_SESSION_TYPE", "wayland")
            .env_remove("SSH_CONNECTION")
            .env_remove("SSH_CLIENT")
            .env_remove("MOSH_CONNECTION")
            .env_remove("TERMUX_VERSION")
            .env_remove("DISPLAY")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{case}: {}\n{}",
            String::from_utf8(output.stdout.clone()).unwrap(),
            String::from_utf8(output.stderr).unwrap()
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let native: serde_json::Value = serde_json::from_str(
            stdout
                .lines()
                .find_map(|line| line.strip_prefix("TRANSFER_RESULT "))
                .expect("transfer result"),
        )
        .unwrap();
        let expected = source.request(&native["request"]);
        assert_eq!(expected["ok"], true, "{case}: {expected}");
        assert_eq!(native["output"]["chat"], expected["output"]["chat"], "{case} chat");
        assert_eq!(
            JsValue::from(&native["output"]),
            JsValue::from(&expected["output"]),
            "{case}"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "invoked by export/copy/share process-boundary parents"]
async fn transfer_command_child() {
    tokio::task::LocalSet::new().run_until(async {
        let case = std::env::var("PI_TRANSFER_CASE").unwrap();
        let cwd = std::env::var("PI_TRANSFER_ROOT").unwrap();
        let root = Path::new(&cwd);
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let tmp = root.join("tmp");
        std::fs::create_dir(&tmp).unwrap();
        let gh = bin.join("gh");
        let script = match case.as_str() {
            "share-auth" => "#!/bin/sh\nexit 1\n".to_owned(),
            "share-error" => "#!/bin/sh\nif [ \"$1\" = auth ]; then exit 0; fi\nprintf '  denied  \\n' >&2\nexit 1\n".to_owned(),
            "share-cancel" => format!("#!/bin/sh\nif [ \"$1\" = auth ]; then exit 0; fi\ntrap 'printf terminated > {:?}; exit 0' TERM\nprintf ready > {:?}\nwhile :; do :; done\n", root.join("terminated"), root.join("ready")),
            _ => "#!/bin/sh\nif [ \"$1\" = auth ]; then exit 0; fi\nprintf 'https://gist.github.com/fixture/abc123\\n'\n".to_owned(),
        };
        if case != "share-missing" { write_transfer_program(&gh, &script); }
        write_transfer_program(&bin.join("which"), if case == "copy-error" { "#!/bin/sh\nexit 1\n" } else { "#!/bin/sh\ntest \"$1\" = wl-copy\n" });
        write_transfer_program(&bin.join("wl-copy"), &format!("#!/usr/bin/python3\nimport pathlib, sys\npathlib.Path({:?}).write_bytes(sys.stdin.buffer.read())\n", root.join("clipboard").to_str().unwrap()));
        let (mode, terminal, faux, _temp) = build_mode_with_extra_providers(
            StaticResourceLoader::new(), None,
            SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default()),
            Vec::new(), serde_json::Map::new(), true, TestModeSetup { cwd: Some(cwd.clone()), ..Default::default() },
        ).await;
        {
            let session = mode.session();
            let mut manager = session.session_manager();
            if case != "copy-none" {
                manager.append_message(pi_agent_core::agent::types::AgentMessage::User(pi_ai::types::UserMessage {
                    role: pi_ai::types::UserRole::default(), content: pi_ai::types::UserContent::Text("Transfer fixture".into()), timestamp: 0.0,
                    input_source: None, provider_native: None,
                })).unwrap();
            }
            if case == "copy-text" || case == "copy-error" {
                let text = if case == "copy-error" { "x".repeat(80_000) } else { "  clipboard text  ".to_owned() };
                manager.append_message(pi_agent_core::agent::types::AgentMessage::Assistant(Box::new(faux_assistant_message(
                    faux_text(text), pi_ai::providers::faux::FauxAssistantMessageOptions::default(),
                )))).unwrap();
            }
            manager.persist_for_recovery().unwrap();
            let (context, identities) = manager.build_session_context_with_identities();
            session.agent().set_messages_with_identities(context.messages, identities).unwrap();
        }
        let colors: serde_json::Map<_, _> = crate::modes::interactive::theme::REQUIRED_COLORS.iter().map(|name| ((*name).to_owned(), serde_json::json!(0))).collect();
        let theme_path = root.join("transfer-theme.json");
        std::fs::write(&theme_path, serde_json::json!({"name":"transfer-fixture","colors":colors}).to_string()).unwrap();
        let theme = crate::modes::interactive::theme::load_theme_from_path(&js_path(&theme_path), Some(crate::modes::interactive::theme::ColorMode::Color256)).unwrap();
        crate::modes::interactive::theme::set_registered_themes(vec![Arc::new(theme)]).unwrap();
        crate::modes::interactive::theme::init_theme(Some(&"transfer-fixture".into()), false).unwrap();
        mode.chat_container.borrow_mut().clear();
        mode.restore_editor();
        mode.ui.start().unwrap();
        let export_path = root.join(if case == "export-jsonl" { "exported branch.jsonl" } else { "exported branch.JSONL" });
        let loader_frame = Rc::new(RefCell::new(Vec::<Vec<u16>>::new()));
        let gh_program = js_path(&gh);
        if case.starts_with("export") {
            mode.handle_export_command(js_format!("/export '{}' ignored", export_path.to_str().unwrap())).await;
        } else if case.starts_with("copy") {
            mode.handle_copy_command().await;
            if case == "copy-text" {
                tokio::time::timeout(Duration::from_secs(5), async {
                    while std::fs::read(root.join("clipboard")).ok().as_deref() != Some(b"clipboard text") { tokio::task::yield_now().await; }
                }).await.expect("fake clipboard received text");
            }
        } else if case == "share-cancel" {
            let drive = async {
                tokio::time::timeout(Duration::from_secs(5), async {
                    while !root.join("ready").exists() { tokio::task::yield_now().await; }
                }).await.expect("fake gh ready");
                *loader_frame.borrow_mut() = mode.editor_container.borrow_mut().render(120).into_iter().map(|line| line.units().to_vec()).collect();
                terminal.borrow_mut().send_input("\u{1b}");
                mode.ui.process_events();
            };
            tokio::join!(mode.handle_share_command_with_program(&gh_program), drive);
        } else {
            mode.handle_share_command_with_program(&gh_program).await;
        }
        let artifact = (case == "export-jsonl").then(|| std::fs::read_to_string(&export_path).unwrap());
        let clock = artifact.as_ref().and_then(|text| text.lines().next()).and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok()).and_then(|header| header["timestamp"].as_str().map(str::to_owned));
        let html = case.starts_with("export") && std::fs::read_to_string(&export_path).unwrap().trim_start().to_ascii_lowercase().starts_with("<!doctype html>");
        let output = serde_json::json!({
            "chat": mode.chat_container.borrow_mut().render(120).into_iter().map(|line| line.units().to_vec()).collect::<Vec<_>>(),
            "loaderFrame": *loader_frame.borrow(),
            "editorFocused": mode.ui.get_focused_component().is_some_and(|focus| Rc::ptr_eq(&focus, &mode.editor_handle.borrow())),
            "temporaryExists": tmp.join("session.html").exists(), "terminated": root.join("terminated").exists(),
            "clipboard": std::fs::read(root.join("clipboard")).unwrap_or_default(), "artifact": artifact, "html": html,
        });
        let request = serde_json::json!({"cwd":cwd,"case":case,"sessionPath":mode.session().session_file().unwrap(),"model":faux.get_model(None).unwrap(),"exportPath":export_path,"clock":clock,"colors":crate::modes::interactive::theme::REQUIRED_COLORS});
        println!("TRANSFER_RESULT {}", serde_json::json!({"request":request,"output":output}));
        mode.stop().unwrap();
    }).await;
}

fn write_transfer_program(path: &Path, source: &str) {
    std::fs::write(path, source).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
