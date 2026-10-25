use pi_coding_agent::core::agent_session::MultiAgentRuntimeRole;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, create_agent_session};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json as json;

fn options(temp: &tempfile::TempDir) -> CreateAgentSessionOptions {
    CreateAgentSessionOptions {
        no_supervisor: Some(true),
        cwd: Some(temp.path().to_str().unwrap().into()),
        agent_dir: Some(temp.path().to_str().unwrap().into()),
        model_registry: Some(ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap()),
        resource_loader: Some(StaticResourceLoader::new()),
        ..Default::default()
    }
}
#[tokio::test]
async fn standalone_sdk_defaults_to_memory_and_blocks_activation_detachment_and_direct_supervisor_calls() {
    let temp = tempfile::TempDir::new().unwrap();
    let result = create_agent_session(options(&temp)).await.unwrap();
    assert!(!result.session.session_manager().is_persisted());
    assert!(result.session.no_supervisor());
    result
        .session
        .set_active_tools_by_name(&["read".into(), "ask_supervisor".into(), "channel_post".into()])
        .unwrap();
    assert!(result.session.get_active_tool_names().contains(&"read".into()));
    assert!(
        !result
            .session
            .get_active_tool_names()
            .contains(&"ask_supervisor".into())
    );
    assert!(!result.session.get_active_tool_names().contains(&"channel_post".into()));
    assert!(!result.session.has_detachable_tool());
    assert!(!result.session.detach_running_tool().expect("fixture detach callback"));
    let context = result.session.extension_runner().create_context();
    assert!(context.no_supervisor().unwrap());
    assert!(context.tool_detach_registry().unwrap().is_none());
    let definition = pi_coding_agent::core::tools::ask_supervisor::create_ask_supervisor_tool_definition();
    let error = (definition.execute)(
        "hidden-advisory".into(),
        json!({"question":"must not contact Supervisor"}),
        None,
        None,
        Some(context.clone()),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.message,
        "Supervisor advisory is unavailable in standalone worker mode"
    );
    let error = pi_coding_agent::bundled_extensions::goal::supervisor_review::review_goal_with_resident_supervisor(
        pi_coding_agent::bundled_extensions::goal::goal_types::GoalReviewInput {
            ctx: context,
            kind: pi_coding_agent::bundled_extensions::goal::goal_types::GoalReviewKind::GoalIdleReview,
            payload: Default::default(),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.message,
        "Supervisor goal review is unavailable in standalone worker mode"
    );
    result.session.dispose();
}
use crate::shared_scripted_registry as scripted_registry;

#[tokio::test]
async fn standalone_llm_approval_blocks_write_without_contacting_supervisor_or_mutating_policy() {
    use pi_ai::providers::faux::{
        FauxAssistantMessageOptions, FauxContent, faux_assistant_message, faux_provider, faux_tool_call,
    };
    use pi_coding_agent::core::permissions::presets::ApprovalPresetName;
    use std::sync::Arc;
    let temp = tempfile::TempDir::new().unwrap();
    let faux = faux_provider(Default::default());
    let model = faux.get_model(None).unwrap();
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::from_iter([(
        model.provider.clone(),
        serde_json::json!({"type":"api_key","key":"fixture"}),
    )])))
    .unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    let mut config = options(&temp);
    config.model = Some(model);
    config.model_registry = Some(runtime);
    config.supervisor_decision_requester = Some(Arc::new(|_| {
        Box::pin(async { panic!("standalone must not contact Supervisor") })
    }));
    let session = create_agent_session(config).await.unwrap().session;
    session
        .settings_manager()
        .set_approval_preset(
            ApprovalPresetName::LlmApprovedDeny,
            pi_coding_agent::core::settings_manager::SettingsScope::Global,
        )
        .unwrap();
    faux.set_responses(vec![
        faux_assistant_message(
            FauxContent::Blocks(vec![faux_tool_call(
                "write",
                json!({"path":"blocked.txt","content":"no"})
                    .as_object()
                    .unwrap()
                    .clone(),
                None,
            )]),
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::StopReason::ToolUse),
                ..Default::default()
            },
        )
        .into(),
        faux_assistant_message("finished", Default::default()).into(),
    ]);
    session.prompt("write the fixture", Default::default()).await.unwrap();
    assert!(!temp.path().join("blocked.txt").exists());
    assert_eq!(
        session.settings_manager().get_approval_preset(),
        ApprovalPresetName::LlmApprovedDeny
    );
    let messages = pi_js_compat::js_json::to_value(&session.messages()).unwrap();
    let result = messages
        .as_array()
        .unwrap()
        .iter()
        .find(|message| {
            message["role"].as_string().is_some_and(|role| role == "toolResult")
                && message["toolName"].as_string().is_some_and(|name| name == "write")
        })
        .expect("blocked write result");
    assert_eq!(result["isError"], pi_js_compat::js_json::JsValue::Bool(true));
    assert!(
        result["content"][0]["text"]
            .as_string()
            .unwrap()
            .contains("Supervisor approval is unavailable in standalone worker mode")
    );
    session.dispose();
}

#[tokio::test]
async fn standalone_sdk_rejects_persisted_or_orchestrated_options_before_runtime_creation() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut config = options(&temp);
    config.session_manager = Some(
        SessionManager::create(
            &temp.path().to_str().unwrap().into(),
            Some(&temp.path().join("sessions").to_str().unwrap().into()),
            None,
        )
        .unwrap(),
    );
    config.cwd = Some("/missing-before-options-validation".into());
    let error = create_agent_session(config)
        .await
        .err()
        .expect("persisted worker rejected");
    assert_eq!(error.message, "Standalone worker requires an in-memory session");
    let mut config = options(&temp);
    config.multi_agent_runtime_role = Some(MultiAgentRuntimeRole::Child);
    let error = create_agent_session(config)
        .await
        .err()
        .expect("orchestrated worker rejected");
    assert_eq!(
        error.message,
        "Standalone worker cannot receive multi-agent orchestration"
    );
}

// Shared support is compiled once per integration target.
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
