use crate::shared_extensions as support;
use pi_ai::InputSource;
use pi_coding_agent::core::agent_session::PromptOptions;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::types::{
    ExtensionContext, HandlerError, InputEvent, InputEventResult, LoadExtensionsResult,
};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::Arc;

fn options(temp: &tempfile::TempDir) -> CreateAgentSessionOptions {
    CreateAgentSessionOptions {
        cwd: Some(temp.path().to_str().unwrap().into()),
        agent_dir: Some(temp.path().to_str().unwrap().into()),
        model_registry: Some(ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap()),
        resource_loader: Some(StaticResourceLoader::new()),
        ..Default::default()
    }
}
fn contexts(session: &pi_coding_agent::core::agent_session::AgentSession) -> Vec<JsValue> {
    let entries = js_json::to_value(&session.session_manager().get_branch(None)).unwrap();
    entries
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| {
            entry["type"].as_string().is_some_and(|kind| kind == "custom")
                && entry["customType"]
                    .as_string()
                    .is_some_and(|kind| kind == "codex_restriction_context")
        })
        .map(|entry| entry["data"].clone())
        .collect()
}
#[tokio::test]
async fn records_original_native_text_before_auth_failure_only_for_explicit_supervised_main_input() {
    for case in 0..6 {
        let temp = tempfile::TempDir::new().unwrap();
        let mut config = options(&temp);
        let source = if case == 1 {
            Some(InputSource::Extension)
        } else {
            Some(InputSource::Rpc)
        };
        if case == 0 {
            config.no_supervisor = Some(true);
        }
        if case == 2 {
            config.multi_agent_agent_id = Some("child".into());
        }
        if case == 3 {
            config.multi_agent_requires_agent_id = true;
        }
        if case == 4 {
            config.multi_agent_agent_id = Some(JsString::default());
        }
        let session = create_agent_session(config).await.unwrap().session;
        let text = JsString::from_utf16(vec![0x61, 0xd800, 0x62]);
        assert!(
            session
                .prompt(
                    text.clone(),
                    PromptOptions {
                        source,
                        ..Default::default()
                    }
                )
                .await
                .is_err()
        );
        let recorded = contexts(&session);
        if case >= 4 {
            assert_eq!(recorded.len(), 1);
            assert_eq!(recorded[0]["userRequest"], JsValue::String(text.clone()));
            let id = recorded[0]["requestId"].as_string().unwrap().to_utf8().unwrap();
            assert!(
                pi_js_compat::js_regex::JsRegex::new(
                    "^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
                )
                .unwrap()
                .is_match(id)
            );
            assert!(
                session
                    .prompt(
                        text,
                        PromptOptions {
                            source,
                            ..Default::default()
                        }
                    )
                    .await
                    .is_err()
            );
            let later = contexts(&session);
            assert_eq!(later.len(), 2);
            assert_ne!(later[0]["requestId"], later[1]["requestId"]);
        } else {
            assert!(recorded.is_empty(), "case {case}");
        }
        session.dispose();
    }
}
#[tokio::test]
async fn handled_input_does_not_record_and_transformed_input_records_pre_transform_text() {
    for handled in [true, false] {
        let temp = tempfile::TempDir::new().unwrap();
        let runtime = support::runtime();
        let extension = support::extension(&runtime, temp.path().to_str().unwrap(), "<fixture:input>", move |api| {
            api.on_input(move |_: InputEvent, _: ExtensionContext| {
                Box::pin(async move {
                    Ok(Some(if handled {
                        InputEventResult::Handled
                    } else {
                        InputEventResult::Transform {
                            text: "rewritten input".into(),
                            images: None,
                        }
                    }))
                }) as pi_ai::request::BoxFuture<'static, Result<Option<InputEventResult>, HandlerError>>
            })
            .unwrap();
        })
        .await;
        let mut config = options(&temp);
        config.resource_loader = Some(Arc::new(StaticResourceLoader {
            extensions: Some(LoadExtensionsResult {
                extensions: vec![extension],
                errors: Vec::new(),
                runtime,
            }),
            ..Default::default()
        }));
        let session = create_agent_session(config).await.unwrap().session;
        session.bind_extensions(Default::default()).await.unwrap();
        let result = session.prompt("original authorized text", Default::default()).await;
        let recorded = contexts(&session);
        if handled {
            assert!(result.is_ok());
            assert!(recorded.is_empty());
        } else {
            assert!(result.is_err());
            assert_eq!(recorded.len(), 1);
            assert_eq!(
                recorded[0]["userRequest"],
                JsValue::String("original authorized text".into())
            );
        }
        session.dispose();
    }
}
