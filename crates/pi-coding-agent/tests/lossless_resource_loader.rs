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
