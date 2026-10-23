#[tokio::test(flavor = "current_thread")]
async fn anthropic_subscription_warning_branches_match_frozen() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    tokio::task::LocalSet::new().run_until(async {
        let mut source=oracle::TsOracle::spawn("interactive-login-oracle.mts","packages/coding-agent/src");
        for case in 0..9 {
            let directory=tempfile::tempdir().unwrap();let path=directory.path().join("models.json");std::fs::write(&path,r#"{"providers":{}}"#).unwrap();
            let credential=if case==3||case==7 {serde_json::json!({"type":"oauth","access":"fixture","refresh":"fixture","expires":9999999999999_f64})}else if case==5 {serde_json::json!({"type":"api_key","key":"ordinary-fixture"})}else if case==6 {serde_json::json!({"type":"api_key","key":{}})}else{serde_json::json!({"type":"api_key","key":"sk-ant-oat-fixture"})};
            let credentials=if case==8 {serde_json::json!({})}else{serde_json::json!({"anthropic":credential})};
            let auth_path=directory.path().join("auth.json");std::fs::write(&auth_path,serde_json::to_vec(&credentials).unwrap()).unwrap();
            let storage=AuthStorage::create(Some(&JsString::from(auth_path.to_str().unwrap()))).unwrap();
            let registry=ModelRegistry::create(storage,Some(JsString::from(path.to_str().unwrap()))).unwrap();
            let (mode,_terminal,_faux,_temp)=build_mode_with_extra_providers(StaticResourceLoader::new(),None,SettingsManager::in_memory(Default::default(),SettingsManagerCreateOptions::default()),Vec::new(),serde_json::Map::new(),false,TestModeSetup {model_registry:Some(registry.clone()),..Default::default()}).await;
            registry.unregister_provider("faux");
            let mut model=mode.session().model();model.provider=if case==2 {"other".into()}else{"anthropic".into()};mode.session().agent().set_model(model.clone());
            let settings=if case==0 {serde_json::json!({"anthropicExtraUsage":false})}else{serde_json::json!({})};
            let native_settings:JsValue=settings.clone().into();mode.settings_mut(|settings|settings.set_warnings(native_settings.as_object().unwrap()));
            let already=case==1;mode.anthropic_subscription_warning_shown.set(already);
            let runtime=if case==7 {Some("ordinary-runtime")}else if case==8 {Some("sk-ant-oat-runtime")}else{None};
            if let Some(key)=runtime {registry.auth_storage().set_runtime_api_key("anthropic",&JsString::from(key));}
            let lookup_failed=if case==6 {registry.get_api_key_for_provider("anthropic").await.is_err()}else{false};
            let use_default=case%2==0;
            let expected=source.request(&serde_json::json!({"id":case,"operation":"warning","modelsPath":path,"authPath":auth_path,"credentials":credentials,"model":model,"warningSettings":settings,"alreadyShown":already,"runtimeKey":runtime,"inspectLookup":case==6,"useDefault":use_default}));
            assert_eq!(expected["ok"],true,"case={case}: {expected}");
            if case==6 {assert_eq!(expected["output"]["lookupFailed"],true);}
            for _ in 0..2 {mode.maybe_warn_about_anthropic_subscription_auth(if use_default {None}else{Some(model.clone())}).await;}
            let chat:Vec<_>=mode.chat_container.borrow_mut().render(200).iter().map(|line|line.units().to_vec()).collect();
            assert_eq!(serde_json::json!({"shown":mode.anthropic_subscription_warning_shown.get(),"lookupFailed":lookup_failed,"chat":chat}),expected["output"],"case={case}");
            mode.stop();
        }
    }).await;
}
