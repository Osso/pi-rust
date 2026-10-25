use pi_coding_agent::core::agent_session_runtime::{
    CreateAgentSessionRuntimeFactory, DefaultRuntimeFactoryOptions, create_agent_session_runtime,
    default_runtime_factory,
};
use pi_coding_agent::core::resource_loader::StaticResourceLoader;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json::JsString;

#[tokio::test]
async fn standalone_runtime_rejects_resume_and_import_before_path_or_factory_work() {
    let temp = tempfile::TempDir::new().unwrap();
    let cwd: JsString = temp.path().to_str().unwrap().into();
    let factory = default_runtime_factory(DefaultRuntimeFactoryOptions {
        resource_loader: StaticResourceLoader::new(),
        model: None,
        thinking_level: None,
        scoped_models: vec![],
        tools: None,
        exclude_tools: None,
        no_tools: None,
        permission_prompt_tool: None,
        custom_tools: vec![],
        base_tools: vec![],
    });
    let manager = SessionManager::in_memory(Some(&cwd), None).unwrap();
    let runtime = create_agent_session_runtime(factory, &cwd, &cwd, manager, Some(true), None)
        .await
        .unwrap();
    assert!(runtime.session().no_supervisor());
    let missing: JsString = "/never-open-this-session".into();
    let error = runtime
        .switch_session(&missing, Default::default())
        .await
        .err()
        .unwrap();
    assert_eq!(error.message, "Session resume is unavailable in standalone worker mode");
    let error = runtime.import_from_jsonl(&missing, None).await.err().unwrap();
    assert_eq!(error.message, "Session import is unavailable in standalone worker mode");
    runtime.session().dispose();
}
#[tokio::test]
async fn standalone_runtime_rejects_persisted_manager_before_cwd_or_factory_work() {
    let temp = tempfile::TempDir::new().unwrap();
    let cwd: JsString = temp.path().to_str().unwrap().into();
    let manager = SessionManager::create(&cwd, Some(&cwd), None).unwrap();
    let factory: CreateAgentSessionRuntimeFactory =
        std::sync::Arc::new(|_| Box::pin(async { panic!("standalone guard must precede factory") }));
    let error = create_agent_session_runtime(factory, &"/missing-cwd".into(), &cwd, manager, Some(true), None)
        .await
        .err()
        .unwrap();
    assert_eq!(error.message, "Standalone worker requires an in-memory session");
}
