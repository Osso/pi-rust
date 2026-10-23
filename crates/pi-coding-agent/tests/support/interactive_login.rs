#[tokio::test(flavor = "current_thread")]
async fn login_command_requires_exact_oauth_id_and_preserves_unknown_text() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    tokio::task::LocalSet::new()
        .run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-login-oracle.mts", "packages/coding-agent/src");
            let inputs = [
                JsString::from("unknown"),
                JsString::from("openai"),
                JsString::from("ANTHROPIC"),
                JsString::from("GITHUB-COPILOT"),
                JsString::from(""),
                JsString::from_utf16(vec![0xd800, 120]),
            ];
            for provider in inputs {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let providers: Vec<_> = mode
                    .session()
                    .auth_storage()
                    .get_oauth_providers()
                    .iter()
                    .map(|info| serde_json::json!({"id":info.id(),"name":info.name()}))
                    .collect();
                let expected = source.request(&serde_json::json!({"provider":provider.units(),"providers":providers}));
                assert_eq!(expected["ok"], true, "{expected}");
                mode.handle_login_command(&provider).await;
                let status = mode.last_status_text.borrow().clone().expect("unknown-ID status");
                let mut status = status.borrow_mut();
                let text = status.as_any_mut().unwrap().downcast_mut::<Text>().unwrap().text();
                let messages = vec![pi_tui::utils::strip_terminal_sequences(text).units().to_vec()];
                assert_eq!(
                    serde_json::json!({"messages":messages,"logins":[]}),
                    expected["output"],
                    "provider={provider:?}"
                );
                mode.stop();
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn login_options_match_frozen_assembly_filtering_order_and_stored_credentials() {
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    tokio::task::LocalSet::new().run_until(async {
        let mut source = oracle::TsOracle::spawn("interactive-login-oracle.mts", "packages/coding-agent/src");
        let (mode, _terminal, _faux, _temp) = build_mode().await;
        let session = mode.session();
        let registry = session.model_registry();
        registry.auth_storage().set("anthropic", serde_json::json!({"type":"oauth","access":"fixture","refresh":"fixture","expires":9999999999999_f64})).unwrap();
        registry.auth_storage().set("openai", serde_json::json!({"type":"api_key","key":"fixture"})).unwrap();
        registry.auth_storage().set("unused", serde_json::json!(false)).unwrap();
        let providers: Vec<_> = registry.auth_storage().get_oauth_providers().iter()
            .map(|info| serde_json::json!({"id":info.id(),"name":info.name()})).collect();
        let model_providers: Vec<_> = registry.get_all().iter().map(|model| model.provider.clone()).collect();
        let credentials: serde_json::Map<String, serde_json::Value> = registry.auth_storage().list_providers().iter()
            .map(|id| (id.to_utf8().unwrap(), serde_json::to_value(registry.auth_storage().get(id).unwrap()).unwrap())).collect();
        let mut names = serde_json::Map::new();
        for id in model_providers.iter().map(String::as_str).chain(credentials.keys().map(String::as_str)) {
            names.insert(id.to_owned(), serde_json::to_value(registry.get_provider_display_name(id)).unwrap());
        }
        let encode = |options: Vec<AuthSelectorProvider>| -> Vec<serde_json::Value> {
            options.into_iter().map(|option| serde_json::json!({"id":option.id,"name":option.name.units(),"authType":option.auth_type.as_str()})).collect()
        };
        for kind in [None, Some(AuthType::Oauth), Some(AuthType::ApiKey)] {
            let expected = source.request(&serde_json::json!({"operation":"options","providers":providers,"modelProviders":model_providers,"credentials":credentials,"names":names,"authType":kind.map(AuthType::as_str)}));
            assert_eq!(expected["ok"], true, "{expected}");
            let actual = serde_json::json!({"login":encode(get_login_provider_options(registry,kind)),"logout":encode(get_logout_provider_options(registry).unwrap())});
            assert_eq!(actual, expected["output"], "kind={kind:?}");
        }
        mode.stop();
    }).await;
}
