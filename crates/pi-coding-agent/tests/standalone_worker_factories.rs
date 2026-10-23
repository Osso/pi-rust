use pi_coding_agent::core::ResourceLoader;
use pi_coding_agent::core::resource_loader::{DefaultResourceLoader, DefaultResourceLoaderOptions};
use pi_coding_agent::main_runner::create_first_party_extension_factories;

#[tokio::test]
async fn standalone_factories_omit_orchestration_and_keep_plain_pyrun_and_small_extensions() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().to_str().unwrap();
    let factories = create_first_party_extension_factories(
        None,
        pi_ai::http::default_fetch(),
        root.into(),
        std::sync::Arc::new(Vec::new),
    );
    let loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: root.into(),
        agent_dir: root.into(),
        extension_factories: factories,
        no_skills: true,
        no_prompt_templates: true,
        no_themes: true,
        no_context_files: true,
        ..Default::default()
    })
    .unwrap();
    loader.reload_with_options(None).await.unwrap();
    let result = loader.get_extensions();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let paths = result
        .extensions
        .iter()
        .map(|extension| extension.read().path.clone())
        .collect::<Vec<_>>();
    for name in ["agents-core", "agent-viewer", "agents-mailbox", "agents-status", "goal"] {
        assert!(
            !paths.contains(&format!("<first-party:{name}>").into()),
            "orchestration extension {name} loaded in standalone mode"
        );
    }
    for name in ["pyrun", "default-footer", "loop", "safe", "approval-controls"] {
        assert!(
            paths.contains(&format!("<first-party:{name}>").into()),
            "standalone must retain {name}"
        );
    }
}
