use pi_coding_agent::core::resource_loader::{
    ResourceLoader, StaticResourceLoader, inline_extension_name, resolve_prompt_input,
};
use pi_js_compat::js_json::JsString;
#[path = "support/oracle.rs"]
mod oracle;

#[test]
fn frozen_context_loading_ignores_override_files() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let agent = temp.path().join("agent");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::write(project.join("AGENTS.md"), "regular").unwrap();
    std::fs::write(project.join("AGENTS.local.md"), "local").unwrap();
    std::fs::write(project.join("AGENTS.override.md"), "old-only").unwrap();
    let files = pi_coding_agent::core::resource_loader::load_project_context_files(
        &project.to_str().unwrap().into(),
        &agent.to_str().unwrap().into(),
    );
    let contents: Vec<JsString> = files.into_iter().map(|file| file.content).collect();
    assert!(contents.contains(&"regular".into()));
    assert!(contents.contains(&"local".into()));
    assert!(!contents.contains(&"old-only".into()));
    let mut source = oracle::TsOracle::spawn(
        "coding-agent-resource-loader-oracle.ts",
        "packages/coding-agent/src/core/resource-loader.ts",
    );
    let expected = source.request(&serde_json::json!({"id": 1, "operation": "context_files", "cwd": project.to_str().unwrap(), "agentDir": agent.to_str().unwrap()}));
    assert_eq!(expected["ok"], true);
    let expected_contents: Vec<JsString> = expected["output"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["content"].as_str().unwrap().into())
        .collect();
    assert_eq!(contents, expected_contents);
}

#[tokio::test]
async fn reload_publishes_resources_in_frozen_order_and_keeps_updates_before_rule_failure() {
    use pi_coding_agent::core::resource_loader::{DefaultResourceLoader, DefaultResourceLoaderOptions};
    use pi_coding_agent::core::settings_manager::SettingsManager;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    let source_root = tempfile::tempdir().unwrap();
    let native_root = tempfile::tempdir().unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "resource-loader-lifecycle-oracle.ts",
        "packages/coding-agent/src/core/resource-loader.ts",
    );
    let expected = oracle.request(&json!({"id":2,"root":source_root.path()}));
    let skill = native_root.path().join("skill");
    std::fs::create_dir(&skill).unwrap();
    let write_skill = |name: &str| {
        std::fs::write(
            skill.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: fixture\n---\n{name} body\n"),
        )
        .unwrap()
    };
    write_skill("first");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let loader_ref: Arc<Mutex<Option<Arc<DefaultResourceLoader>>>> = Arc::new(Mutex::new(None));
    let from_override = Arc::clone(&loader_ref);
    let seen_override = Arc::clone(&seen);
    let loader = Arc::new(
        DefaultResourceLoader::new(DefaultResourceLoaderOptions {
            cwd: native_root.path().to_str().unwrap().into(),
            agent_dir: native_root.path().to_str().unwrap().into(),
            settings_manager: Some(Arc::new(Mutex::new(SettingsManager::in_memory(
                Default::default(),
                Default::default(),
            )))),
            no_extensions: true,
            no_skills: true,
            no_prompt_templates: true,
            no_themes: true,
            no_context_files: true,
            additional_skill_paths: vec![skill.to_str().unwrap().into()],
            prompts_override: Some(Arc::new(move |base| {
                let loader = from_override.lock().unwrap().clone().unwrap();
                seen_override.lock().unwrap().push(
                    loader
                        .get_skills()
                        .skills
                        .into_iter()
                        .map(|skill| skill.name)
                        .collect::<Vec<_>>(),
                );
                base
            })),
            ..Default::default()
        })
        .unwrap(),
    );
    *loader_ref.lock().unwrap() = Some(Arc::clone(&loader));
    loader.reload().await.unwrap();
    write_skill("second");
    std::fs::create_dir_all(native_root.path().join("rules/bad.md")).unwrap();
    let error = loader.reload().await.unwrap_err();
    assert_eq!(
        json!({"id":2,"seen":seen.lock().unwrap().clone(),"skills":loader.get_skills().skills.into_iter().map(|skill|skill.name).collect::<Vec<_>>(),"error":error}),
        expected
    );
    loader_ref.lock().unwrap().take();
}

#[test]
fn extension_resource_metadata_path_errors_match_frozen() {
    use pi_coding_agent::core::package_manager::{MetadataOrigin, PathMetadata, SourceScope};
    use pi_coding_agent::core::resource_loader::{
        DefaultResourceLoader, DefaultResourceLoaderOptions, ExtendedResourcePath, ResourceExtensionPaths,
    };
    use serde_json::json;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("prompt.md");
    std::fs::write(&path, "prompt content").unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "resource-loader-lifecycle-oracle.ts",
        "packages/coding-agent/src/core/resource-loader.ts",
    );
    let expected = oracle.request(
        &json!({"id":3,"operation":"extend_path","root":root.path(),"path":path,"baseDir":"file://remote/resources"}),
    );
    let loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: root.path().to_str().unwrap().into(),
        agent_dir: root.path().to_str().unwrap().into(),
        ..Default::default()
    })
    .unwrap();
    let error = loader
        .extend_resources(ResourceExtensionPaths {
            prompt_paths: vec![ExtendedResourcePath {
                path: path.to_str().unwrap().into(),
                metadata: PathMetadata {
                    source: "extension".into(),
                    scope: SourceScope::Temporary,
                    origin: MetadataOrigin::TopLevel,
                    base_dir: Some("file://remote/resources".into()),
                },
            }],
            ..Default::default()
        })
        .err();
    assert_eq!(json!({"id":3,"error":error}), expected);
}

#[tokio::test]
async fn inline_factory_errors_use_frozen_non_error_fallback_text() {
    use pi_ai::request::CallbackError;
    use pi_coding_agent::core::extensions::types::HandlerError;
    use pi_coding_agent::core::resource_loader::{
        DefaultResourceLoader, DefaultResourceLoaderOptions, InlineExtension,
    };
    use pi_js_compat::{abort::AbortReason, js_json};
    use std::sync::Arc;
    let root = tempfile::tempdir().unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "resource-loader-lifecycle-oracle.ts",
        "packages/coding-agent/src/core/resource-loader.ts",
    );
    let values = vec![js_json!("string rejection"), js_json!({"why":"failed"}), js_json!(null)];
    let expected = oracle
        .request(&js_json!({"id":4,"operation":"factory_errors","root":root.path().to_str().unwrap(),"values":values}));
    let factories = values
        .into_iter()
        .map(|value| InlineExtension::Factory {
            factory: Arc::new(move |_| {
                let error = HandlerError::from(CallbackError::from_rejection(AbortReason::Value(value.clone())));
                Box::pin(async move { Err(error) })
            }),
            extension_path: None,
        })
        .collect();
    let loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: root.path().to_str().unwrap().into(),
        agent_dir: root.path().to_str().unwrap().into(),
        no_extensions: true,
        no_skills: true,
        no_prompt_templates: true,
        no_themes: true,
        no_context_files: true,
        extension_factories: factories,
        ..Default::default()
    })
    .unwrap();
    loader.reload().await.unwrap();
    assert_eq!(js_json!({"id":4,"errors":loader.get_extensions().errors}), expected);
}

#[test]
fn extension_resources_propagate_invalid_path_errors() {
    use pi_coding_agent::core::package_manager::{MetadataOrigin, PathMetadata, SourceScope};
    use pi_coding_agent::core::resource_loader::{
        DefaultResourceLoader, DefaultResourceLoaderOptions, ExtendedResourcePath, ResourceExtensionPaths,
    };
    let temp = tempfile::tempdir().unwrap();
    let root = JsString::from(temp.path().to_str().unwrap());
    let loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: root.clone(),
        agent_dir: root,
        ..Default::default()
    })
    .unwrap();
    let result = loader.extend_resources(ResourceExtensionPaths {
        skill_paths: vec![ExtendedResourcePath {
            path: "file://remote/skills".into(),
            metadata: PathMetadata {
                source: "extension".into(),
                scope: SourceScope::Temporary,
                origin: MetadataOrigin::TopLevel,
                base_dir: None,
            },
        }],
        ..Default::default()
    });
    assert!(result.is_err());
}

#[test]
fn literal_prompts_and_inline_names_keep_unpaired_units() {
    let raw = JsString::from_utf16(vec![0xd800, 0xdc00, 0xd800]);
    assert_eq!(resolve_prompt_input(Some(&raw), "system prompt"), Some(raw.clone()));
    let inline = JsString::from("<first-party:") + &raw + ">";
    assert_eq!(inline_extension_name(&inline), Some(raw.clone()));
    let loader = StaticResourceLoader {
        system_prompt: Some(raw.clone()),
        append_system_prompt: vec![raw.clone()],
        ..Default::default()
    };
    assert_eq!(loader.get_system_prompt(), Some(raw.clone()));
    assert_eq!(loader.get_append_system_prompt(), vec![raw]);
}
