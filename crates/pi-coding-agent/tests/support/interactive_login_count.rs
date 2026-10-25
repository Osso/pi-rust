#[tokio::test(flavor = "current_thread")]
async fn authentication_completion_refreshes_registry_and_counts_unique_scoped_providers() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    tokio::task::LocalSet::new().run_until(async {
        let mut source=oracle::TsOracle::spawn("interactive-login-oracle.mts","packages/coding-agent/src");
        let scopes=[vec![],vec!["scope-a","scope-a"],vec!["scope-a","scope-b"],vec!["scope-a","scope-a","scope-b","π"]];
        for (case,providers) in scopes.into_iter().enumerate() {
            let directory=tempfile::tempdir().unwrap();let path=directory.path().join("models.json");
            let provider_config=serde_json::json!({"api":"openai-completions","baseUrl":"https://fixture.invalid","apiKey":"fixture","models":[{"id":"count-model","name":"Count model","reasoning":false,"input":["text"],"contextWindow":4096,"maxTokens":512,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}}]});
            std::fs::write(&path,serde_json::to_vec(&serde_json::json!({"providers":{"count-a":provider_config}})).unwrap()).unwrap();
            let registry=ModelRegistry::create(AuthStorage::in_memory(Vec::<(String,serde_json::Value)>::new()),Some(JsString::from(path.to_str().unwrap()))).unwrap();
            let (mode,_terminal,_faux,_temp)=build_mode_with_extra_providers(
                StaticResourceLoader::new(),None,
                SettingsManager::in_memory(Default::default(),SettingsManagerCreateOptions::default()),
                Vec::new(),serde_json::Map::new(),false,
                TestModeSetup {model_registry:Some(registry.clone()),..Default::default()},
            ).await;
            registry.unregister_provider("faux").unwrap();
            let previous=mode.session().model();
            let scoped:Vec<_>=providers.into_iter().map(|provider| {
                let mut model=previous.clone();model.id="same-id".into();model.provider=provider.into();
                crate::core::model_resolver::ScopedModel {model,thinking_level:Some(ThinkingLevel::Off)}
            }).collect();
            mode.session().set_scoped_models(scoped.clone());
            let wire_scoped:Vec<_>=scoped.iter().map(|entry|serde_json::json!({"model":entry.model,"thinkingLevel":"off"})).collect();
            mode.settings_mut(|settings|settings.set_warnings(pi_js_compat::js_json!({"anthropicExtraUsage":false}).as_object().unwrap()));
            // Refresh must observe the second provider added after registry construction.
            std::fs::write(&path,serde_json::to_vec(&serde_json::json!({"providers":{"count-a":provider_config,"count-b":provider_config}})).unwrap()).unwrap();
            let name=if case==1 {JsString::default()} else {JsString::from_utf16(vec![67,111,117,110,116,32,0xd800])};
            let kind=if case%2==0 {LoginAuthType::ApiKey}else{LoginAuthType::Oauth};
            let expected=source.request(&serde_json::json!({"operation":"complete","modelsPath":path,"credentials":{},"provider":"count-a","name":name.units(),"authType":kind.as_str(),"previous":previous,"scopedModels":wire_scoped}));
            assert_eq!(expected["ok"],true,"{expected}");
            mode.complete_provider_authentication("count-a",&name,kind,previous).await.unwrap();
            assert_eq!(authentication_ui_snapshot(&mode),expected["output"],"case={case}");
            assert!(registry.find("count-b","count-model").is_some());
            mode.stop().unwrap();
        }
    }).await;
}
