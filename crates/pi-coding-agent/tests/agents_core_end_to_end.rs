//! End-to-end multi-agent orchestration: a real parent `AgentSession` spawns a
//! real child session through the production factory against a faux provider
//! and a temporary control database.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, RegisterFauxProviderOptions,
    faux_assistant_message, faux_provider, faux_tool_call,
};
use pi_ai::types::StopReason;
use pi_coding_agent::bundled_extensions::agents_core::runtime::{
    MultiAgentExtensionOptions, create_multi_agent_runtime_handles, multi_agent_extension,
};
use pi_coding_agent::bundled_extensions::agents_core::session_factories::{
    AgentSessionChildSession, ChildExtensionFactories, CreateChildSession, CreateChildSessionManager,
    ProductionChildAgentSessionFactoryOptions, UnboundChildAgentSession, create_production_child_agent_session_factory,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::types::HandlerError;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, MultiAgentStore, MultiAgentStoreOptions, SessionPersistence,
};
use pi_coding_agent::core::resource_loader::InlineExtension;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};
use pi_coding_agent::core::session_control_db::{get_control_db_path, read_multi_agent_agent};
use pi_coding_agent::core::session_manager::{SessionManager, lock_session_manager};
use pi_js_compat::js_json as json;

struct TestPersistence {
    control_db_path: JsString,
    session_path: JsString,
}

impl SessionPersistence for TestPersistence {
    fn metadata_control_db_path(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.control_db_path.clone())
    }

    fn session_file(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.session_path.clone())
    }
}

fn faux_model() -> FauxModelDefinition {
    FauxModelDefinition {
        id: "faux-1".to_owned(),
        name: Some("Faux Model".into()),
        reasoning: Some(true),
        input: None,
        cost: None,
        context_window: Some(100_000.0),
        max_tokens: Some(4096.0),
    }
}

async fn model_registry(faux: &FauxProviderHandle) -> ModelRegistry {
    let mut data = pi_js_compat::js_json::JsObject::new();
    data.insert(JsString::from("faux"), json!({"type": "api_key", "key": "faux-key"}));
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(data)).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    runtime
}

#[tokio::test]
async fn production_factory_spawns_and_completes_a_real_child_session() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path().join("repo").display().to_string();
    std::fs::create_dir_all(&cwd).expect("cwd");
    let agent_dir = dir.path().join("agent").display().to_string();
    std::fs::create_dir_all(&agent_dir).expect("agent dir");
    std::fs::write(
        dir.path().join("agent").join("settings.json"),
        pi_js_compat::js_json::stringify_lossless(
            &json!({"agents": {"explore": {"model": "faux/faux-1", "thinkingLevel": "off"}}}),
        ),
    )
    .expect("settings");
    let session_dir = dir.path().join("sessions").display().to_string();
    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path(dir.path().as_os_str()));

    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![faux_model()]),
        ..Default::default()
    });
    let runtime = model_registry(&faux).await;
    let model = faux.get_model(None).expect("model");

    let mut parent_manager = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(session_dir)), None)
        .expect("parent manager");
    parent_manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db_path)), Default::default())
        .expect("metadata control db");
    let parent_session_path = parent_manager.get_session_file().expect("session file").to_owned();

    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store.set_persistence_session_manager(Some(&TestPersistence {
        control_db_path: control_db_path.clone(),
        session_path: parent_session_path.clone(),
    }));
    let handles = create_multi_agent_runtime_handles();

    let child_runtime = runtime.clone();
    let create_session: CreateChildSession = Arc::new(move |mut options: CreateAgentSessionOptions| {
        let runtime = child_runtime.clone();
        Box::pin(async move {
            options.model_registry = Some(runtime);
            let result = create_agent_session(options).await.map_err(|error| error.message)?;
            Ok(Arc::new(AgentSessionChildSession::new(result.session)) as Arc<dyn UnboundChildAgentSession>)
        })
    });
    let create_session_manager: CreateChildSessionManager = Arc::new(|cwd, session_dir, options| {
        SessionManager::create(cwd, session_dir, Some(options)).map_err(|error| error.message().clone())
    });
    let child_factory = create_production_child_agent_session_factory(ProductionChildAgentSessionFactoryOptions {
        browser_agent_config_root: None,
        agent_dir: Some(agent_dir.clone().into()),
        create_session,
        create_session_manager,
        extension_factories: Some(ChildExtensionFactories::Static(Vec::new())),
        multi_agent_store: Some(Arc::clone(&store)),
        session_dir: None,
    });

    let options = MultiAgentExtensionOptions {
        create_child_session: Some(child_factory),
        runtime_handles: Some(handles.clone()),
        store: Some(Arc::clone(&store)),
        ..Default::default()
    };
    let factory = Arc::new(move |api: &pi_coding_agent::core::extensions::ExtensionApi| {
        let options = options.clone();
        let registered = multi_agent_extension(api, &options);
        Box::pin(async move { registered })
            as pi_coding_agent::core::extensions::BoxFuture<'static, Result<(), HandlerError>>
    });

    let result = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.clone().into()),
        agent_dir: Some(agent_dir.into()),
        model_registry: Some(runtime),
        model: Some(model),
        thinking_level: Some(ThinkingLevel::Off),
        tools: Some(vec!["spawn_agent".to_owned()]),
        session_manager: Some(parent_manager),
        control_db_path: Some(control_db_path.clone()),
        multi_agent_store: Some(Arc::clone(&store)),
        extension_factories: vec![InlineExtension::Factory {
            extension_path: Some("<first-party:agents-core>".to_owned().into()),
            factory,
        }],
        ..Default::default()
    })
    .await
    .expect("parent session");
    assert!(
        result.extensions_result.errors.is_empty(),
        "{:?}",
        result.extensions_result.errors
    );
    let parent = result.session;
    parent
        .bind_extensions(pi_coding_agent::core::agent_session::ExtensionBindings::default())
        .await
        .expect("bind parent extensions");

    faux.set_responses(vec![
        faux_assistant_message("child finished the task", FauxAssistantMessageOptions::default()).into(),
        faux_assistant_message(
            faux_tool_call(
                "end_turn",
                json!({"reason": "done"}).as_object().cloned().expect("object"),
                None,
            ),
            FauxAssistantMessageOptions {
                stop_reason: Some(StopReason::ToolUse),
                ..Default::default()
            },
        )
        .into(),
    ]);

    let definition = parent
        .extension_runner()
        .get_tool_definition("spawn_agent")
        .expect("spawn_agent definition");
    let tool_result = (definition.execute)(
        "tool-1".to_owned(),
        json!({"prompt": "Inspect the repository and report the module layout", "agentType": "explore", "context": "fresh"}),
        None,
        None,
        Some(parent.extension_runner().create_context()),
    )
    .await
    .expect("spawn_agent tool");
    let details = tool_result.details.expect("spawn details");
    let agent_id = details["agent"]["id"].as_string().expect("agent id").to_owned();
    assert_eq!(details["dispatched"], json!(true));

    let dispatch = handles
        .dispatches
        .lock()
        .expect("dispatches")
        .get(&agent_id)
        .cloned()
        .expect("dispatch future");
    let completed = dispatch.await.expect("dispatch result");
    assert_eq!(completed.lifecycle, AgentLifecycle::Completed);
    assert_eq!(
        completed.result.as_ref().and_then(|result| result.summary.as_ref()),
        Some(&JsString::from("child finished the task"))
    );

    let persisted = read_multi_agent_agent(&control_db_path, &parent_session_path, &agent_id)
        .expect("read agent")
        .expect("persisted agent");
    assert_eq!(persisted.lifecycle, AgentLifecycle::Completed);
    let transcript = persisted.transcript.expect("transcript");
    let child_session_file = transcript.path.expect("child session file");
    assert!(
        std::path::Path::new(&child_session_file.to_utf8().expect("utf-8 test path")).exists(),
        "{child_session_file:?}"
    );
    let child_manager = SessionManager::open(&child_session_file, None, None).expect("child session");
    let user_texts: Vec<JsString> = child_manager
        .get_entries()
        .into_iter()
        .filter_map(
            |entry| match entry.known().expect("fixture entry projection").into_owned() {
                pi_coding_agent::core::session_manager::KnownSessionEntry::Message(message) => match *message.message {
                    pi_agent_core::agent::types::AgentMessage::User(user) => match user.content {
                        pi_ai::types::UserContent::Text(text) => Some(text),
                        pi_ai::types::UserContent::Blocks(blocks) => Some(JsString::join(
                            blocks
                                .iter()
                                .filter_map(|block| match block {
                                    pi_ai::types::TextOrImageContent::Text(text) => Some(text.text.clone()),
                                    pi_ai::types::TextOrImageContent::Image(_) => None,
                                })
                                .collect::<Vec<_>>(),
                            "",
                        )),
                    },
                    _ => None,
                },
                _ => None,
            },
        )
        .collect();
    assert!(
        user_texts
            .iter()
            .any(|text| text.contains("Inspect the repository and report the module layout")),
        "{user_texts:?}"
    );

    let journal_types: Vec<JsString> = lock_session_manager(&parent.session_manager_handle())
        .get_entries()
        .into_iter()
        .filter_map(
            |entry| match entry.known().expect("fixture entry projection").into_owned() {
                pi_coding_agent::core::session_manager::KnownSessionEntry::Custom(custom) => Some(custom.custom_type),
                _ => None,
            },
        )
        .collect();
    assert!(
        journal_types.is_empty(),
        "frozen runtime writes no parent agent journal: {journal_types:?}"
    );
}

#[path = "support/scripted_registry.rs"]
mod scripted_registry;
