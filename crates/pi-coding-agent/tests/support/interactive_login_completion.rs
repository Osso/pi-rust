#[tokio::test(flavor = "current_thread")]
async fn authentication_completion_inherited_defaults_match_frozen_real_models_json() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    tokio::task::LocalSet::new().run_until(async {
        let mut source=oracle::TsOracle::spawn("interactive-login-oracle.mts","packages/coding-agent/src");
        let properties=source.request(&serde_json::json!({"operation":"prototype"}));
        assert_eq!(properties["ok"],true,"{properties}");
        for property in properties["output"]["properties"].as_array().unwrap() {
            let id=property["name"].as_str().unwrap();
            let inherited_text=property["text"].as_str().unwrap();
            let directory=tempfile::tempdir().unwrap();let path=directory.path().join("models.json");
            // A string ID equal to String(inheritedValue) must still not match the inherited object/function.
            let config=serde_json::json!({"providers":{(id):{"api":"openai-completions","baseUrl":"https://fixture.invalid","apiKey":"fixture","models":[{"id":inherited_text,"name":"Prototype probe","reasoning":false,"input":["text"],"contextWindow":4096,"maxTokens":512,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}}]}}});
            std::fs::write(&path,serde_json::to_vec(&config).unwrap()).unwrap();
            let registry=ModelRegistry::create(AuthStorage::in_memory(Vec::<(String,serde_json::Value)>::new()),Some(JsString::from(path.to_str().unwrap()))).unwrap();
            assert_eq!(registry.get_error(),None);
            assert!(registry.find(id,inherited_text).is_some());
            let (mode,_terminal,_faux,_temp)=build_mode_with_extra_providers(
                StaticResourceLoader::new(),None,
                SettingsManager::in_memory(Default::default(),SettingsManagerCreateOptions::default()),
                Vec::new(),serde_json::Map::new(),false,
                TestModeSetup {model_registry:Some(registry.clone()),..Default::default()},
            ).await;
            registry.unregister_provider("faux");
            let mut previous=mode.session().model();previous.id="unknown".into();previous.provider="unknown".into();previous.api="unknown".into();previous.name="Unknown".into();
            mode.session().agent().set_model(previous.clone());
            mode.settings_mut(|settings|settings.set_warnings(pi_js_compat::js_json!({"anthropicExtraUsage":false}).as_object().unwrap()));
            let name=JsString::from_utf16(vec![80,114,111,118,105,100,101,114,32,0xd800]);
            let expected=source.request(&serde_json::json!({"operation":"complete","modelsPath":path,"credentials":{},"provider":id,"name":name.units(),"authType":"api_key","previous":previous}));
            assert_eq!(expected["ok"],true,"id={id}: {expected}");
            mode.complete_provider_authentication(id,&name,AuthType::ApiKey,previous).await.unwrap();
            let status=mode.last_status_text.borrow().clone().unwrap();
            let mut messages=Vec::<Vec<u16>>::new();let mut errors=Vec::<Vec<u16>>::new();
            for child in mode.chat_container.borrow().children() {
                let mut component=child.borrow_mut();
                if let Some(text)=component.as_any_mut().and_then(|value|value.downcast_mut::<Text>()) {
                    let value=pi_tui::utils::strip_terminal_sequences(text.text()).units().to_vec();
                    if Rc::ptr_eq(child,&status) {messages.push(value);}else{errors.push(value);}
                }
            }
            let actual=serde_json::json!({"model":mode.session().model(),"count":mode.footer_data_provider.get_available_provider_count(),"messages":messages,"errors":errors});
            assert_eq!(actual,expected["output"],"provider={id}");
            mode.stop();
        }
    }).await;
}
