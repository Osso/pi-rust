#[tokio::test(flavor = "current_thread")]
async fn authentication_completion_ordinary_defaults_match_frozen() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    use crate::core::model_registry::ProviderConfigInput;
    tokio::task::LocalSet::new().run_until(async {
        let mut source=oracle::TsOracle::spawn("interactive-login-oracle.mts","packages/coding-agent/src");
        for case in 0..5 {
            let directory=tempfile::tempdir().unwrap();let path=directory.path().join("models.json");
            std::fs::write(&path,r#"{"providers":{}}"#).unwrap();
            let registry=ModelRegistry::create(AuthStorage::in_memory(Vec::<(String,serde_json::Value)>::new()),Some(JsString::from(path.to_str().unwrap()))).unwrap();
            let (mode,_terminal,_faux,_temp)=build_mode_with_extra_providers(StaticResourceLoader::new(),None,SettingsManager::in_memory(Default::default(),SettingsManagerCreateOptions::default()),Vec::new(),serde_json::Map::new(),case==4,TestModeSetup {model_registry:Some(registry.clone()),..Default::default()}).await;
            registry.unregister_provider("faux");
            let provider=if case==0 {"q15-ordinary"}else{"openai-codex"};
            let mut model=mode.session().model();model.provider=provider.into();model.api="openai-completions".into();model.base_url="https://fixture.invalid".into();model.name="Ordinary fixture".into();model.reasoning=false;
            model.id=if case>=3 {crate::core::model_resolver::default_model_for_provider(provider).unwrap().into()}else{"not-default".into()};
            let mut overrides=Vec::new();
            if case!=1 {
                registry.register_provider(provider,ProviderConfigInput {api:Some(model.api.clone()),base_url:Some(model.base_url.clone()),api_key:Some("fixture".into()),models:Some(vec![model.clone()]),..Default::default()}).unwrap();
                overrides.push(serde_json::json!({"id":provider,"config":{"api":model.api,"baseUrl":model.base_url,"apiKey":"fixture","models":[model]}}));
            }
            let mut previous=mode.session().model();previous.id="unknown".into();previous.provider="unknown".into();previous.api="unknown".into();previous.name="Unknown".into();mode.session().agent().set_model(previous.clone());
            mode.settings_mut(|settings|settings.set_warnings(pi_js_compat::js_json!({"anthropicExtraUsage":false}).as_object().unwrap()));
            let name=JsString::from_utf16(vec![79,114,100,105,110,97,114,121,32,0xd800]);let kind=if case%2==0 {LoginAuthType::ApiKey}else{LoginAuthType::Oauth};
            let failed_metadata_path = if case == 4 {
                let parent = directory.path().join("blocked-db-parent");
                std::fs::write(&parent, "not a directory").unwrap();
                let database = JsString::from(parent.join("control.sqlite").to_str().unwrap());
                mode.session().session_manager().set_metadata_control_db_path(Some(&database), Default::default()).unwrap_err();
                Some(database)
            } else {
                None
            };
            let setter_error = if failed_metadata_path.is_some() {
                let error = mode.session().set_model(model.clone()).await.unwrap_err();
                let native = CallbackError::from(error).thrown_value().as_error().unwrap().value();
                mode.session().agent().set_model(previous.clone());
                Some(native.to_serde().unwrap())
            } else {
                None
            };
            let expected=source.request(&serde_json::json!({"id":case,"operation":"complete","modelsPath":path,"credentials":{},"provider":provider,"providerOverrides":overrides,"name":name.units(),"authType":kind.as_str(),"previous":previous,"failedMetadataPath":failed_metadata_path,"selectionRoot":directory.path(),"selectionModel":model}));
            assert_eq!(expected["ok"],true,"case={case}: {expected}");
            mode.complete_provider_authentication(provider,&name,kind,previous).await.unwrap();
            let mut actual = authentication_ui_snapshot(&mode);
            if let Some(error) = setter_error {
                actual["setterError"] = error;
            }
            assert_eq!(actual, expected["output"], "case={case}");
            if case!=1 {registry.unregister_provider(provider);}
            if failed_metadata_path.is_some() {
                mode.session().session_manager().set_metadata_control_db_path(None, Default::default()).unwrap();
            }
            mode.stop().unwrap();
        }
    }).await;
}
