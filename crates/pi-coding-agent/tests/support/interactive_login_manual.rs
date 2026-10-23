#[path = "env_probe.rs"]
mod manual_env_probe;

#[test]
fn oauth_callback_server_manual_input_lifetime_matches_frozen() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let launcher = temp.path().join("xdg-open");
    std::fs::write(&launcher, "#!/usr/bin/python3\nimport sys\nsys.exit(0)\n").unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", temp.path().display(), std::env::var("PATH").unwrap());
    manual_env_probe::run_probe(
        "modes::interactive::interactive_mode::ui_tests::oauth_manual_code_probe",
        &[
            ("PATH", path.into()),
            ("HOME", temp.path().join("home").into_os_string()),
            ("XDG_CONFIG_HOME", temp.path().join("config").into_os_string()),
            ("XDG_STATE_HOME", temp.path().join("state").into_os_string()),
            ("XDG_CACHE_HOME", temp.path().join("cache").into_os_string()),
        ],
    );
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "isolated browser-stub probe"]
async fn oauth_manual_code_probe() {
    if !manual_env_probe::in_probe() {
        return;
    }
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    use crate::core::model_registry::{ExtensionOAuthConfig, ProviderConfigInput};
    use pi_ai::auth::AuthError;
    tokio::task::LocalSet::new().run_until(async {
        let mut source=oracle::TsOracle::spawn("interactive-login-callback-oracle.mts","packages/coding-agent/src");
        for callback_server in [false,true] {
            let code=JsString::from_utf16(vec![113,49,53,0xd800]);
            let expected=source.request(&serde_json::json!({"operation":"manual","callbackServer":callback_server,"code":code.units()}));
            assert_eq!(expected["ok"],true,"{expected}");
            let (mode,_terminal,_faux,_temp)=build_mode().await;
            let entered=Arc::new(tokio::sync::Notify::new());
            let release=Arc::new(tokio::sync::Notify::new());
            let choices=Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
            let captured=choices.clone(); let login_entered=entered.clone(); let login_release=release.clone();
            mode.session().model_registry().register_provider("q15-callback",ProviderConfigInput {
                oauth:Some(ExtensionOAuthConfig {
                    name:"Fixture".into(),uses_callback_server:Some(callback_server),
                    login:Arc::new(move |callbacks| {
                        let entered=login_entered.clone(); let release=login_release.clone(); let captured=captured.clone();
                        Box::pin(async move {
                            callbacks.on_auth(pi_ai::OAuthAuthInfo {url:"https://fixture.invalid/auth".into(),instructions:None});
                            entered.notify_one(); release.notified().await;
                            if callback_server {
                                for _ in 0..2 {
                                    let value=callbacks.on_manual_code_input().unwrap().await.map_err(AuthError::Failed)?;
                                    captured.lock().unwrap().push(serde_json::json!(value.units()));
                                }
                            }
                            Err(AuthError::failed("fixture stop"))
                        })
                    }),
                    refresh_token:Arc::new(|_,_|Box::pin(async {Err(AuthError::failed("unexpected refresh"))})),
                    get_api_key:Arc::new(|credential|Ok(credential.access.clone())),modify_models:None,
                }),..Default::default()
            }).unwrap();
            let running=tokio::task::spawn_local({let mode=mode.clone();async move {mode.show_login_dialog("q15-callback",&"Fixture".into()).await}});
            tokio::time::timeout(Duration::from_secs(5),entered.notified()).await.unwrap();
            mode.poll_auth_ui();
            let frames=mode.editor_container.borrow_mut().render(80);
            assert_eq!(serde_json::json!([frames]),expected["output"]["frames"],"early onAuth callbackServer={callback_server}");
            release.notify_one();
            if callback_server {
                tokio::task::yield_now().await;
                mode.poll_auth_ui();
                let dialog=mode.auth_ui.borrow().as_ref().unwrap().dialog.clone();
                dialog.borrow_mut().handle_input(&code);dialog.borrow_mut().handle_input(&"\r".into());
            }
            tokio::time::timeout(Duration::from_secs(5),running).await.unwrap().unwrap();
            let errors:Vec<_>=mode.chat_container.borrow_mut().render(200).into_iter().map(|line|pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line|!line.is_empty()).map(|line|line.units().to_vec()).collect();
            let restored=mode.editor_container.borrow().children().iter().any(|child|Rc::ptr_eq(child,&mode.editor_handle.borrow()));
            assert_eq!(serde_json::json!({"choices":*choices.lock().unwrap(),"frames":[frames],"errors":errors,"restored":restored}),expected["output"],"callbackServer={callback_server}");
            mode.session().model_registry().unregister_provider("q15-callback");mode.stop();
        }
    }).await;
}
