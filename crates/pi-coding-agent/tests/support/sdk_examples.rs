use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxResponseStep, RegisterFauxProviderOptions, faux_assistant_message, faux_provider,
    faux_text, faux_tool_call,
};
use pi_coding_agent::core::{
    auth_storage::AuthStorage, model_registry::ModelRegistry, resource_loader::StaticResourceLoader,
    sdk::CreateAgentSessionOptions, session_manager::SessionManager, settings_manager::SettingsManager,
};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct Output(pub Arc<Mutex<Vec<u8>>>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Output {
    pub fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

pub struct Fixture {
    pub home: tempfile::TempDir,
    pub options: CreateAgentSessionOptions,
    pub requests: Arc<Mutex<Vec<Value>>>,
}
pub async fn fixture() -> Fixture {
    let home = tempfile::tempdir().unwrap();
    let cwd = home.path().join("project");
    let agent_dir = home.path().join("agent");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    let faux = faux_provider(RegisterFauxProviderOptions {
        models: Some(vec![pi_ai::providers::faux::FauxModelDefinition {
            id: "sdk-fixture".into(),
            name: Some("SDK fixture".into()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(100_000.0),
            max_tokens: Some(4096.0),
        }]),
        ..Default::default()
    });
    let model = faux.get_model(None).unwrap();
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::from_iter([(
        model.provider.clone(),
        json!({"type":"api_key","key":"fixture"}),
    )])))
    .unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&requests);
    let step = FauxResponseStep::Factory(Arc::new(move |context, options, _, model| {
        seen.lock()
            .unwrap()
            .push(json!({"context":context,"model":model,"reasoning":options.and_then(|o| o.reasoning)}));
        Box::pin(async {
            Ok(faux_assistant_message(
                vec![
                    faux_text("Hello!"),
                    faux_tool_call(
                        "end_turn",
                        serde_json::Map::from_iter([("reason".into(), json!("done"))])
                            .into_iter()
                            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                            .collect::<pi_js_compat::js_json::JsObject>(),
                        Some("done".into()),
                    ),
                ],
                FauxAssistantMessageOptions {
                    timestamp: Some(42.0),
                    ..Default::default()
                },
            ))
        })
    }));
    faux.set_responses(vec![step.clone(), step.clone(), step.clone(), step]);
    let options = CreateAgentSessionOptions {
        cwd: Some(JsString::from(cwd.display().to_string())),
        agent_dir: Some(JsString::from(agent_dir.display().to_string())),
        model: Some(model),
        model_registry: Some(runtime),
        resource_loader: Some(Arc::new(StaticResourceLoader::default())),
        session_manager: Some(
            SessionManager::in_memory(Some(&JsString::from(&cwd.display().to_string())), None).unwrap(),
        ),
        settings_manager: Some(Arc::new(Mutex::new(SettingsManager::in_memory(
            serde_json::Map::new()
                .into_iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect::<pi_js_compat::js_json::JsObject>(),
            Default::default(),
        )))),
        control_db_path: Some(JsString::from(home.path().join("control.sqlite").display().to_string())),
        disable_runtime_coordination_inbound: true,
        ..Default::default()
    };
    Fixture {
        home,
        options,
        requests,
    }
}

use crate::shared_scripted_registry as scripted_registry;
