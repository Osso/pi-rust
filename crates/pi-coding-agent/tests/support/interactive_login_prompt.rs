#[tokio::test(flavor = "current_thread")]
async fn oauth_prompt_device_and_progress_boundaries_match_frozen() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    use crate::core::model_registry::{ExtensionOAuthConfig, ProviderConfigInput};
    use pi_ai::auth::AuthError;
    tokio::task::LocalSet::new().run_until(async {
        let mut source=oracle::TsOracle::spawn("interactive-login-callback-oracle.mts","packages/coding-agent/src");
        for case in 0..3 {
            let message=JsString::from_utf16(vec![80,114,111,109,112,116,32,0xd800]);
            let progress=JsString::from_utf16(vec![87,111,114,107,32,0xdc00]);
            let placeholder=match case {0=>None,1=>Some(JsString::default()),_=>Some(JsString::from_utf16(vec![0xd800]))};
            let code=if case==1 {JsString::default()} else {JsString::from_utf16(vec![97,0xd800])};
            let cancel=case==2;
            let expected=source.request(&serde_json::json!({"operation":"prompt","message":message.units(),"progress":progress.units(),"placeholder":placeholder.as_ref().map(JsString::units),"code":code.units(),"cancel":cancel}));
            assert_eq!(expected["ok"],true,"{expected}");
            let (mode,_terminal,_faux,_temp)=build_mode().await;
            let entered=Arc::new(tokio::sync::Notify::new()); let release=Arc::new(tokio::sync::Notify::new());
            let login_entered=entered.clone(); let login_release=release.clone();
            let choices=Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new())); let captured=choices.clone();
            mode.session().model_registry().register_provider("q15-callback",ProviderConfigInput {
                oauth:Some(ExtensionOAuthConfig {
                    name:"Fixture".into(),uses_callback_server:Some(false),
                    login:Arc::new(move |callbacks| {
                        let entered=login_entered.clone(); let release=login_release.clone(); let captured=captured.clone();
                        let message=message.clone();let progress=progress.clone();let placeholder=placeholder.clone();
                        Box::pin(async move {
                            callbacks.on_device_code(pi_ai::OAuthDeviceCodeInfo {user_code:"ABCD".into(),verification_uri:"https://fixture.invalid/device".into(),interval_seconds:Some(0.0),expires_in_seconds:Some(-0.0)})?;
                            entered.notify_one();release.notified().await;
                            callbacks.on_progress(&progress)?;
                            entered.notify_one();release.notified().await;
                            let result=callbacks.on_prompt(pi_ai::OAuthPrompt {message,placeholder,allow_empty:Some(false)}).await;
                            let value=match result {Ok(value)=>serde_json::json!({"ok":value.units()}),Err(error)=>serde_json::json!({"error":error.message.units()})};
                            captured.lock().unwrap().push(value);
                            Err(AuthError::failed("fixture stop"))
                        })
                    }),
                    refresh_token:Arc::new(|_,_|Box::pin(async {Err(AuthError::failed("unexpected refresh"))})),
                    get_api_key:Arc::new(|credential|Ok(credential.access.clone())),modify_models:None,
                }),..Default::default()
            }).unwrap();
            let running=tokio::task::spawn_local({let mode=mode.clone();async move {mode.show_login_dialog("q15-callback",&"Fixture".into()).await}});
            let mut frames=Vec::<Vec<Vec<u16>>>::new();
            for _ in 0..2 {
                tokio::time::timeout(Duration::from_secs(5),entered.notified()).await.unwrap();
                mode.poll_auth_ui();
                frames.push(mode.editor_container.borrow_mut().render(80).iter().map(|line|line.units().to_vec()).collect());
                release.notify_one();
            }
            let deadline=std::time::Instant::now()+Duration::from_secs(5);
            loop {
                mode.poll_auth_ui();
                let frame=mode.editor_container.borrow_mut().render(80);
                if frame.iter().any(|line|line.contains("Prompt")) {frames.push(frame.iter().map(|line|line.units().to_vec()).collect());break;}
                assert!(std::time::Instant::now()<deadline,"prompt did not mount");
                tokio::task::yield_now().await;
            }
            let dialog=mode.auth_ui.borrow().as_ref().unwrap().dialog.clone();
            if cancel {dialog.borrow_mut().handle_input(&"\u{1b}".into());}
            else {dialog.borrow_mut().handle_input(&code);dialog.borrow_mut().handle_input(&"\r".into());}
            tokio::time::timeout(Duration::from_secs(5),running).await.unwrap().unwrap();
            let chat:Vec<_>=mode.chat_container.borrow_mut().render(200).iter().map(|line|line.units().to_vec()).collect();
            let restored=mode.editor_container.borrow().children().iter().any(|child|Rc::ptr_eq(child,&mode.editor_handle.borrow()));
            assert_eq!(serde_json::json!({"choices":*choices.lock().unwrap(),"frames":frames,"chat":chat,"restored":restored}),expected["output"],"case={case}");
            mode.session().model_registry().unregister_provider("q15-callback").unwrap();mode.stop().unwrap();
        }
    }).await;
}
