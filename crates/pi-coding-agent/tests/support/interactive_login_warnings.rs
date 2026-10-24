#[tokio::test(flavor = "current_thread")]
async fn anthropic_subscription_warning_branches_match_frozen() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    tokio::task::LocalSet::new()
        .run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-login-oracle.mts", "packages/coding-agent/src");
            for case in 0..9 {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("models.json");
                std::fs::write(&path, r#"{"providers":{}}"#).unwrap();
                let credentials = warning_test_credentials(case);
                let auth_path = directory.path().join("auth.json");
                std::fs::write(&auth_path, serde_json::to_vec(&credentials).unwrap()).unwrap();
                let storage = AuthStorage::create(Some(&JsString::from(auth_path.to_str().unwrap()))).unwrap();
                let registry = ModelRegistry::create(storage, Some(JsString::from(path.to_str().unwrap()))).unwrap();
                let (mode, _terminal, _faux, _temp) = build_mode_with_extra_providers(
                    StaticResourceLoader::new(),
                    None,
                    SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default()),
                    Vec::new(),
                    serde_json::Map::new(),
                    false,
                    TestModeSetup {
                        model_registry: Some(registry.clone()),
                        ..Default::default()
                    },
                )
                .await;
                registry.unregister_provider("faux");
                let mut model = mode.session().model();
                model.provider = if case == 2 { "other".into() } else { "anthropic".into() };
                mode.session().agent().set_model(model.clone());
                let settings = if case == 0 {
                    serde_json::json!({"anthropicExtraUsage": false})
                } else {
                    serde_json::json!({})
                };
                let native_settings: JsValue = settings.clone().into();
                mode.settings_mut(|settings| settings.set_warnings(native_settings.as_object().unwrap()));
                let already = case == 1;
                mode.anthropic_subscription_warning_shown.set(already);
                let runtime = match case {
                    7 => Some("ordinary-runtime"),
                    8 => Some("sk-ant-oat-runtime"),
                    _ => None,
                };
                if let Some(key) = runtime {
                    registry
                        .auth_storage()
                        .set_runtime_api_key("anthropic", &JsString::from(key));
                }
                let lookup_failed = if case == 6 {
                    registry.get_api_key_for_provider("anthropic").await.is_err()
                } else {
                    false
                };
                let use_default = case % 2 == 0;
                let expected = source.request(&serde_json::json!({
                    "id": case, "operation": "warning", "modelsPath": path, "authPath": auth_path,
                    "credentials": credentials, "model": model, "warningSettings": settings,
                    "alreadyShown": already, "runtimeKey": runtime, "inspectLookup": case == 6,
                    "useDefault": use_default,
                }));
                assert_eq!(expected["ok"], true, "case={case}: {expected}");
                if case == 6 {
                    assert_eq!(expected["output"]["lookupFailed"], true);
                }
                for _ in 0..2 {
                    let explicit_model = if use_default { None } else { Some(model.clone()) };
                    mode.maybe_warn_about_anthropic_subscription_auth(explicit_model).await;
                }
                let mut actual = authentication_warning_snapshot(&mode);
                actual["lookupFailed"] = lookup_failed.into();
                assert_eq!(actual, expected["output"], "case={case}");

                if matches!(case, 3 | 4) {
                    mode.chat_container.borrow_mut().clear();
                    mode.anthropic_subscription_warning_shown.set(false);
                    let expected = source.request(&serde_json::json!({
                        "id": case, "operation": "warningCompletion", "modelsPath": path,
                        "authPath": auth_path, "model": model,
                    }));
                    assert_eq!(expected["ok"], true, "completion case={case}: {expected}");
                    mode.complete_provider_authentication(
                        "anthropic",
                        &JsString::from("Anthropic"),
                        LoginAuthType::ApiKey,
                        model.clone(),
                    )
                    .await
                    .unwrap();
                    tokio::task::yield_now().await;
                    let after = authentication_warning_snapshot(&mode);
                    assert_eq!(after, expected["output"]["after"], "settled completion case={case}");
                }
                mode.stop();
            }
        })
        .await;
}

fn warning_test_credentials(case: usize) -> serde_json::Value {
    let credential = match case {
        3 | 7 => serde_json::json!({
            "type": "oauth", "access": "fixture", "refresh": "fixture", "expires": 9999999999999_f64,
        }),
        5 => serde_json::json!({"type": "api_key", "key": "ordinary-fixture"}),
        6 => serde_json::json!({"type": "api_key", "key": {}}),
        8 => return serde_json::json!({}),
        _ => serde_json::json!({"type": "api_key", "key": "sk-ant-oat-fixture"}),
    };
    serde_json::json!({"anthropic": credential})
}

fn authentication_warning_snapshot(mode: &InteractiveMode) -> serde_json::Value {
    let chat: Vec<_> = mode
        .chat_container
        .borrow_mut()
        .render(200)
        .iter()
        .map(|line| line.units().to_vec())
        .collect();
    serde_json::json!({"shown": mode.anthropic_subscription_warning_shown.get(), "chat": chat})
}
