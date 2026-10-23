#[tokio::test(flavor = "current_thread")]
async fn oauth_select_cancel_and_empty_ids_match_frozen_callback_values() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    use crate::core::model_registry::{ExtensionOAuthConfig, ProviderConfigInput};
    use pi_ai::auth::AuthError;
    tokio::task::LocalSet::new().run_until(async {
        let mut source = oracle::TsOracle::spawn("interactive-login-callback-oracle.mts", "packages/coding-agent/src");
        for keys in [vec!["\u{1b}"], vec!["\r"], vec!["\u{1b}[B", "\r"]] {
            let expected = source.request(&serde_json::json!({"keys":keys}));
            assert_eq!(expected["ok"], true, "{expected}");
            let (mode, _terminal, _faux, _temp) = build_mode().await;
            let choices = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
            let captured = choices.clone();
            mode.session().model_registry().register_provider("q15-callback", ProviderConfigInput {
                oauth: Some(ExtensionOAuthConfig {
                    name: "Fixture".into(), uses_callback_server: Some(false),
                    login: Arc::new(move |callbacks| {
                        let captured = captured.clone();
                        Box::pin(async move {
                            let choice = callbacks.on_select(pi_ai::OAuthSelectPrompt {
                                message: "Choose fixture".into(), options: vec![
                                    pi_ai::OAuthSelectOption {id:"".into(),label:"Empty ID".into()},
                                    pi_ai::OAuthSelectOption {id:"other".into(),label:"Other".into()},
                                ],
                            }).await;
                            let value = match choice {
                                Ok(choice) => serde_json::json!({"defined":choice.is_some(),"units":choice.as_ref().map(JsString::units)}),
                                Err(error) => serde_json::json!({"error":error.message.units()}),
                            };
                            captured.lock().unwrap().push(value);
                            Err(AuthError::failed("fixture stop"))
                        })
                    }),
                    refresh_token: Arc::new(|_,_| Box::pin(async {Err(AuthError::failed("unexpected refresh"))})),
                    get_api_key: Arc::new(|credential| Ok(credential.access.clone())), modify_models: None,
                }), ..Default::default()
            }).unwrap();
            let running = tokio::task::spawn_local({let mode=mode.clone(); async move {mode.show_login_dialog("q15-callback", &"Fixture".into()).await}});
            let deadline = std::time::Instant::now()+Duration::from_secs(5);
            let frames = loop {
                mode.poll_auth_ui();
                let frames: Vec<_> = mode.editor_container.borrow_mut().render(80).into_iter()
                    .map(|line| pi_tui::utils::strip_terminal_sequences(&line)).collect();
                if frames.iter().any(|line| line.contains("Choose fixture")) {break frames;}
                assert!(std::time::Instant::now()<deadline,"selector did not mount");
                tokio::task::yield_now().await;
            };
            for key in &keys {
                let focus=mode.editor_container.borrow().children()[0].clone();
                focus.borrow_mut().handle_input(&JsString::from(*key));
            }
            tokio::time::timeout(Duration::from_secs(5),running).await.unwrap().unwrap();
            let errors: Vec<_> = mode.chat_container.borrow_mut().render(200).into_iter()
                .map(|line| pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line|!line.is_empty())
                .map(|line|line.units().to_vec()).collect();
            let restored=mode.editor_container.borrow().children().iter().any(|child|Rc::ptr_eq(child,&mode.editor_handle.borrow()));
            assert_eq!(serde_json::json!({"choices":*choices.lock().unwrap(),"frames":[frames],"errors":errors,"restored":restored}), expected["output"],"keys={keys:?}");
            mode.session().model_registry().unregister_provider("q15-callback");
            mode.stop();
        }
    }).await;
}
