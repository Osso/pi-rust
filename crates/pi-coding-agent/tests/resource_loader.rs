use pi_js_compat::js_json::JsString;
use std::fs;

use pi_coding_agent::core::resource_loader::{first_party_extension_name, inline_extension_name, resolve_prompt_input};
use tempfile::TempDir;

#[test]
fn names_first_party_and_inline_extension_paths() {
    assert_eq!(
        first_party_extension_name(&JsString::from("<first-party:browser>")),
        Some(JsString::from("browser"))
    );
    assert_eq!(first_party_extension_name(&JsString::from("<inline>")), None);
    assert_eq!(first_party_extension_name(&JsString::from("<first-party:broken")), None);

    assert_eq!(
        inline_extension_name(&JsString::from("<first-party:browser>")),
        Some(JsString::from("browser"))
    );
    assert_eq!(
        inline_extension_name(&JsString::from("<inline>")),
        Some(JsString::from("inline"))
    );
    assert_eq!(inline_extension_name(&JsString::from("/tmp/extension.ts")), None);
    assert_eq!(inline_extension_name(&JsString::from("<>")), Some(JsString::from("")));
}

#[test]
fn resolves_prompt_input_files_and_literals() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("SYSTEM.md");
    fs::write(&path, "System prompt text.\n").unwrap();

    assert_eq!(
        resolve_prompt_input(Some(&JsString::from(path.to_str().unwrap())), "system prompt").as_ref(),
        Some(&JsString::from("System prompt text.\n"))
    );
    assert_eq!(
        resolve_prompt_input(Some(&JsString::from("Literal prompt text.")), "system prompt").as_ref(),
        Some(&JsString::from("Literal prompt text."))
    );
    assert_eq!(resolve_prompt_input(None, "system prompt"), None);
}

use std::path::Path;
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::event_bus::create_event_bus;
use pi_coding_agent::core::extensions::loader::ExtensionFactory;
use pi_coding_agent::core::extensions::types::{ExtensionApi, FlagType, ToolDefinition};
use pi_coding_agent::core::resource_loader::{
    DefaultResourceLoader, DefaultResourceLoaderOptions, ExtendedResourcePath, InlineExtension, ResourceExtensionPaths,
    ResourceLoader, RulesScope, load_project_context_files,
};
use pi_coding_agent::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};
use pi_coding_agent::package_manager_cli::ProjectTrustExtensionsLoader;
use serde_json::json;

fn settings(cwd: &Path, agent_dir: &Path, trusted: bool) -> Arc<Mutex<SettingsManager>> {
    Arc::new(Mutex::new(
        SettingsManager::create(
            &JsString::from(cwd.to_str().unwrap()),
            Some(&JsString::from(agent_dir.to_str().unwrap())),
            SettingsManagerCreateOptions {
                project_trusted: Some(trusted),
            },
        )
        .unwrap(),
    ))
}

fn loader(cwd: &Path, agent_dir: &Path, trusted: bool) -> DefaultResourceLoader {
    DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: cwd.to_string_lossy().into_owned().into(),
        agent_dir: agent_dir.to_string_lossy().into_owned().into(),
        settings_manager: Some(settings(cwd, agent_dir, trusted)),
        ..Default::default()
    })
    .unwrap()
}

fn write_skill(dir: &Path, name: &str, description: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\nSkill content."),
    )
    .unwrap();
}

fn theme_json(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let mut value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    value["name"] = json!(name);
    serde_json::to_string_pretty(&value).unwrap()
}

#[tokio::test]
async fn starts_empty_and_discovers_skills_and_prompts() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();

    let resource_loader = loader(&cwd, &agent_dir, true);
    assert!(resource_loader.get_extensions().extensions.is_empty());
    assert!(resource_loader.get_skills().skills.is_empty());
    assert!(resource_loader.get_prompts().prompts.is_empty());
    assert!(resource_loader.get_themes().themes.is_empty());

    write_skill(&agent_dir.join("skills/test-skill"), "test-skill", "A test skill");
    std::fs::create_dir_all(agent_dir.join("prompts")).unwrap();
    std::fs::write(
        agent_dir.join("prompts/test-prompt.md"),
        "---\ndescription: A test prompt\n---\nPrompt.",
    )
    .unwrap();

    resource_loader.reload().await.unwrap();
    let skills = resource_loader.get_skills().skills;
    assert!(skills.iter().any(|skill| skill.name == "test-skill"));
    let prompts = resource_loader.get_prompts().prompts;
    assert!(prompts.iter().any(|prompt| prompt.name == "test-prompt"));
    assert_eq!(
        prompts
            .iter()
            .find(|prompt| prompt.name == "test-prompt")
            .unwrap()
            .source_info
            .source,
        "auto"
    );
}

#[tokio::test]
async fn project_resources_win_name_collisions() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(agent_dir.join("prompts")).unwrap();
    std::fs::create_dir_all(cwd.join(".pi/prompts")).unwrap();
    std::fs::write(agent_dir.join("prompts/commit.md"), "User prompt").unwrap();
    std::fs::write(cwd.join(".pi/prompts/commit.md"), "Project prompt").unwrap();
    write_skill(&agent_dir.join("skills/collision"), "collision-skill", "user");
    write_skill(&cwd.join(".pi/skills/collision"), "collision-skill", "project");
    std::fs::create_dir_all(agent_dir.join("themes")).unwrap();
    std::fs::create_dir_all(cwd.join(".pi/themes")).unwrap();
    std::fs::write(agent_dir.join("themes/collision.json"), theme_json("collision-theme")).unwrap();
    std::fs::write(cwd.join(".pi/themes/collision.json"), theme_json("collision-theme")).unwrap();

    let resource_loader = loader(&cwd, &agent_dir, true);
    resource_loader.reload().await.unwrap();

    let prompt = resource_loader
        .get_prompts()
        .prompts
        .into_iter()
        .find(|prompt| prompt.name == "commit")
        .unwrap();
    assert_eq!(
        prompt.file_path,
        JsString::from(cwd.join(".pi/prompts/commit.md").to_str().unwrap())
    );
    let skill = resource_loader
        .get_skills()
        .skills
        .into_iter()
        .find(|skill| skill.name == "collision-skill")
        .unwrap();
    assert_eq!(
        skill.file_path,
        JsString::from(cwd.join(".pi/skills/collision/SKILL.md").to_str().unwrap())
    );
    let themes = resource_loader.get_themes();
    let theme = themes
        .themes
        .iter()
        .find(|theme| theme.name.as_ref() == Some(&JsString::from("collision-theme")))
        .unwrap();
    assert_eq!(
        theme.source_path.as_ref(),
        Some(&JsString::from(cwd.join(".pi/themes/collision.json").to_str().unwrap()))
    );
    assert_eq!(themes.diagnostics.len(), 1);
    assert_eq!(themes.diagnostics[0].message, "name \"collision-theme\" collision");
    assert!(
        resource_loader
            .get_prompts()
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "name \"/commit\" collision")
    );
    assert!(
        resource_loader
            .get_skills()
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("collision-skill"))
    );
}

#[tokio::test]
async fn agents_context_files_layer_with_memory() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    let nested = cwd.join("service");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir_all(agent_dir.join("docs/local")).unwrap();
    std::fs::create_dir_all(cwd.join("docs/local")).unwrap();
    std::fs::write(agent_dir.join("AGENTS.md"), "global instructions").unwrap();
    std::fs::write(agent_dir.join("AGENTS.override.md"), "global override").unwrap();
    std::fs::write(agent_dir.join("docs/local/memory.md"), "Global memory must not load.").unwrap();
    std::fs::write(cwd.join("AGENTS.md"), "project instructions").unwrap();
    std::fs::write(cwd.join("docs/local/memory.md"), "Project memory.").unwrap();
    std::fs::write(nested.join("AGENTS.md"), "service instructions").unwrap();
    std::fs::write(nested.join("AGENTS.override.md"), "service override").unwrap();

    let resource_loader = loader(&nested, &agent_dir, true);
    resource_loader.reload().await.unwrap();
    let files = resource_loader.get_agents_files().agents_files;
    let contents: Vec<JsString> = files.iter().map(|file| file.content.clone()).collect();
    assert_eq!(
        contents,
        vec![
            "global instructions",
            "project instructions",
            "Project memory.",
            "service instructions"
        ]
    );
}

#[tokio::test]
async fn agents_family_suppresses_claude_family_across_directories() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    let nested = cwd.join("src");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(cwd.join("CLAUDE.md"), "Ancestor Claude context.").unwrap();
    std::fs::write(nested.join("AGENTS.md"), "Nested agents context.").unwrap();

    let resource_loader = loader(&nested, &agent_dir, true);
    resource_loader.reload().await.unwrap();
    assert_eq!(
        resource_loader.get_agents_files().agents_files,
        vec![pi_coding_agent::core::system_prompt::ContextFile {
            path: nested.join("AGENTS.md").to_string_lossy().into_owned().into(),
            content: "Nested agents context.".to_owned().into(),
        }]
    );
}

#[tokio::test]
async fn claude_family_loads_when_no_agents_family_exists() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    let nested = cwd.join("src");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(cwd.join("CLAUDE.md"), "Ancestor Claude context.").unwrap();
    std::fs::write(nested.join("CLAUDE.local.md"), "Nested Claude context.").unwrap();

    let resource_loader = loader(&nested, &agent_dir, true);
    resource_loader.reload().await.unwrap();
    let contents: Vec<JsString> = resource_loader
        .get_agents_files()
        .agents_files
        .into_iter()
        .map(|file| file.content)
        .collect();
    assert_eq!(contents, vec!["Ancestor Claude context.", "Nested Claude context."]);
}

#[tokio::test]
async fn context_file_variants_dedupe_and_directories_are_ignored() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(cwd.join("AGENTS.md"), "Shared context.").unwrap();
    std::os::unix::fs::symlink(cwd.join("AGENTS.md"), cwd.join("AGENTS.MD")).unwrap();
    let directory_cwd = temp.path().join("directory-candidates");
    std::fs::create_dir_all(directory_cwd.join("AGENTS.md")).unwrap();
    std::fs::create_dir_all(directory_cwd.join("AGENTS.override.md")).unwrap();
    std::fs::write(directory_cwd.join("CLAUDE.md"), "Fallback instructions").unwrap();

    let resource_loader = loader(&cwd, &agent_dir, true);
    resource_loader.reload().await.unwrap();
    let files = resource_loader.get_agents_files().agents_files;
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, JsString::from(cwd.join("AGENTS.md").to_str().unwrap()));

    let nested_loader = loader(&directory_cwd, &agent_dir, true);
    nested_loader.reload().await.unwrap();
    let nested_files = nested_loader.get_agents_files().agents_files;
    assert!(nested_files.iter().any(|file| file.content == "Fallback instructions"));
}

#[tokio::test]
async fn no_context_files_skips_all_discovery() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(cwd.join("docs/local")).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(cwd.join("AGENTS.md"), "Project instructions.").unwrap();
    std::fs::write(cwd.join("docs/local/memory.md"), "Project memory.").unwrap();

    let resource_loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: cwd.to_string_lossy().into_owned().into(),
        agent_dir: agent_dir.to_string_lossy().into_owned().into(),
        settings_manager: Some(settings(&cwd, &agent_dir, true)),
        no_context_files: true,
        ..Default::default()
    })
    .unwrap();
    resource_loader.reload().await.unwrap();
    assert!(resource_loader.get_agents_files().agents_files.is_empty());
}

#[tokio::test]
async fn rules_load_in_sorted_order_and_respect_scope() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    let rules_dir = agent_dir.join("rules");
    std::fs::create_dir_all(rules_dir.join("main")).unwrap();
    std::fs::create_dir_all(rules_dir.join("child")).unwrap();
    std::fs::write(rules_dir.join("20-second.md"), "\nSecond rule.\n").unwrap();
    std::fs::write(rules_dir.join("10-first.md"), "\nFirst rule.\n").unwrap();
    std::fs::write(rules_dir.join("empty.md"), "   \n").unwrap();
    std::fs::write(rules_dir.join("ignore.txt"), "Ignored rule.").unwrap();
    std::fs::write(rules_dir.join("main/coordination.md"), "Main rule.").unwrap();
    std::fs::write(rules_dir.join("child/worker.md"), "Child rule.").unwrap();

    let main_loader = loader(&cwd, &agent_dir, true);
    main_loader.reload().await.unwrap();
    assert_eq!(
        main_loader.get_rules_content().as_ref(),
        Some(&JsString::from("First rule.\n\nSecond rule.\n\nMain rule."))
    );
    assert_eq!(main_loader.get_rules_files().rules_files.len(), 3);

    let child_loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: cwd.to_string_lossy().into_owned().into(),
        agent_dir: agent_dir.to_string_lossy().into_owned().into(),
        settings_manager: Some(settings(cwd.as_path(), &agent_dir, true)),
        rules_scope: Some(RulesScope::Child),
        ..Default::default()
    })
    .unwrap();
    child_loader.reload().await.unwrap();
    assert_eq!(
        child_loader.get_rules_content().as_ref(),
        Some(&JsString::from("First rule.\n\nSecond rule.\n\nChild rule."))
    );
}

#[tokio::test]
async fn project_rules_load_only_when_trusted() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(agent_dir.join("rules")).unwrap();
    std::fs::create_dir_all(cwd.join(".pi/rules")).unwrap();
    std::fs::write(agent_dir.join("rules/global.md"), "Global rule.").unwrap();
    std::fs::write(cwd.join(".pi/rules/project.md"), "Project rule.").unwrap();

    let untrusted = loader(&cwd, &agent_dir, false);
    untrusted.reload().await.unwrap();
    assert_eq!(
        untrusted.get_rules_content().as_ref(),
        Some(&JsString::from("Global rule."))
    );

    let trusted = loader(&cwd, &agent_dir, true);
    trusted.reload().await.unwrap();
    assert_eq!(
        trusted.get_rules_content().as_ref(),
        Some(&JsString::from("Global rule.\n\nProject rule."))
    );
}

#[tokio::test]
async fn rules_reload_from_disk() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(agent_dir.join("rules")).unwrap();
    std::fs::write(agent_dir.join("rules/rule.md"), "First rule.").unwrap();

    let resource_loader = loader(&cwd, &agent_dir, true);
    resource_loader.reload().await.unwrap();
    assert_eq!(
        resource_loader.get_rules_content().as_ref(),
        Some(&JsString::from("First rule."))
    );

    std::fs::write(agent_dir.join("rules/rule.md"), "Updated rule.").unwrap();
    resource_loader.reload().await.unwrap();
    assert_eq!(
        resource_loader.get_rules_content().as_ref(),
        Some(&JsString::from("Updated rule."))
    );
}

#[tokio::test]
async fn system_and_append_prompt_discovery_and_sources() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(cwd.join(".pi")).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(cwd.join(".pi/SYSTEM.md"), "Project system prompt.").unwrap();
    std::fs::write(cwd.join(".pi/APPEND_SYSTEM.md"), "Project append prompt.").unwrap();
    std::fs::write(agent_dir.join("SYSTEM.md"), "Global system prompt.").unwrap();

    let resource_loader = loader(&cwd, &agent_dir, true);
    resource_loader.reload().await.unwrap();
    assert_eq!(
        resource_loader.get_system_prompt().as_ref(),
        Some(&JsString::from("Project system prompt."))
    );
    assert_eq!(
        resource_loader.get_append_system_prompt(),
        vec!["Project append prompt."]
    );

    let untrusted = loader(&cwd, &agent_dir, false);
    untrusted.reload().await.unwrap();
    assert_eq!(
        untrusted.get_system_prompt().as_ref(),
        Some(&JsString::from("Global system prompt."))
    );
    assert!(untrusted.get_append_system_prompt().is_empty());
}

#[tokio::test]
async fn literal_prompts_are_not_exposed_as_sources() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    let custom = temp.path().join("custom-system.md");
    std::fs::write(&custom, "Custom system prompt.").unwrap();
    let append = temp.path().join("custom-append.md");
    std::fs::write(&append, "Custom append prompt.").unwrap();

    let resource_loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: cwd.to_string_lossy().into_owned().into(),
        agent_dir: agent_dir.to_string_lossy().into_owned().into(),
        settings_manager: Some(settings(&cwd, &agent_dir, true)),
        system_prompt: Some(custom.to_string_lossy().into_owned().into()),
        append_system_prompt: Some(vec![
            append.to_string_lossy().into_owned().into(),
            "Literal append prompt.".to_owned().into(),
        ]),
        ..Default::default()
    })
    .unwrap();
    resource_loader.reload().await.unwrap();
    assert_eq!(
        resource_loader.get_system_prompt().as_ref(),
        Some(&JsString::from("Custom system prompt."))
    );
    assert_eq!(
        resource_loader.get_append_system_prompt(),
        vec!["Custom append prompt.", "Literal append prompt."]
    );

    let literal = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: cwd.to_string_lossy().into_owned().into(),
        agent_dir: agent_dir.to_string_lossy().into_owned().into(),
        settings_manager: Some(settings(&cwd, &agent_dir, true)),
        system_prompt: Some("Literal system prompt.".to_owned().into()),
        ..Default::default()
    })
    .unwrap();
    literal.reload().await.unwrap();
    assert_eq!(
        literal.get_system_prompt().as_ref(),
        Some(&JsString::from("Literal system prompt."))
    );
}

#[tokio::test]
async fn no_skills_still_loads_additional_skill_paths() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    write_skill(&agent_dir.join("skills/test-skill"), "test-skill", "A test skill");
    let custom = temp.path().join("custom-skills");
    write_skill(&custom.join("custom"), "custom", "Custom skill");

    let resource_loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: cwd.to_string_lossy().into_owned().into(),
        agent_dir: agent_dir.to_string_lossy().into_owned().into(),
        settings_manager: Some(settings(&cwd, &agent_dir, true)),
        no_skills: true,
        additional_skill_paths: vec![custom.to_string_lossy().into_owned().into()],
        ..Default::default()
    })
    .unwrap();
    resource_loader.reload().await.unwrap();
    let skills = resource_loader.get_skills().skills;
    assert!(skills.iter().any(|skill| skill.name == "custom"));
    assert!(!skills.iter().any(|skill| skill.name == "test-skill"));
}

#[tokio::test]
async fn overrides_replace_loaded_resources() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();

    let injected = pi_coding_agent::core::skills::Skill {
        name: "injected".to_owned().into(),
        description: "Injected skill".to_owned().into(),
        file_path: "/fake/path".to_owned().into(),
        base_dir: "/fake".to_owned().into(),
        source_info: pi_coding_agent::core::source_info::create_synthetic_source_info(
            &JsString::from("/fake/path"),
            &pi_coding_agent::core::source_info::SyntheticSourceInfoOptions {
                source: "custom".to_owned().into(),
                scope: None,
                origin: None,
                base_dir: None,
            },
        ),
        disable_model_invocation: false,
    };
    let resource_loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: cwd.to_string_lossy().into_owned().into(),
        agent_dir: agent_dir.to_string_lossy().into_owned().into(),
        settings_manager: Some(settings(&cwd, &agent_dir, true)),
        skills_override: Some(Arc::new(move |_| {
            pi_coding_agent::core::resource_loader::LoadedSkills {
                skills: vec![injected.clone()],
                diagnostics: Vec::new(),
            }
        })),
        system_prompt_override: Some(Arc::new(|_| Some("Custom system prompt".to_owned().into()))),
        ..Default::default()
    })
    .unwrap();
    resource_loader.reload().await.unwrap();
    assert_eq!(resource_loader.get_skills().skills.len(), 1);
    assert_eq!(resource_loader.get_skills().skills[0].name, "injected");
    assert_eq!(
        resource_loader.get_system_prompt().as_ref(),
        Some(&JsString::from("Custom system prompt"))
    );
}

#[tokio::test]
async fn extend_resources_loads_skills_and_prompts_with_metadata() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    let extra_skill = temp.path().join("extra-skills/extra-skill");
    write_skill(&extra_skill, "extra-skill", "Extra skill");
    let extra_prompts = temp.path().join("extra-prompts");
    std::fs::create_dir_all(&extra_prompts).unwrap();
    std::fs::write(
        extra_prompts.join("extra.md"),
        "---\ndescription: Extra prompt\n---\nExtra prompt content",
    )
    .unwrap();

    let resource_loader = loader(&cwd, &agent_dir, true);
    resource_loader.reload().await.unwrap();
    resource_loader
        .extend_resources(ResourceExtensionPaths {
            skill_paths: vec![ExtendedResourcePath {
                path: extra_skill.to_string_lossy().into_owned().into(),
                metadata: pi_coding_agent::core::package_manager::PathMetadata {
                    source: "extension:extra".to_owned().into(),
                    scope: pi_coding_agent::core::package_manager::SourceScope::Temporary,
                    origin: pi_coding_agent::core::package_manager::MetadataOrigin::TopLevel,
                    base_dir: Some(extra_skill.to_string_lossy().into_owned().into()),
                },
            }],
            prompt_paths: vec![ExtendedResourcePath {
                path: JsString::from(extra_prompts.join("extra.md").to_str().unwrap()),
                metadata: pi_coding_agent::core::package_manager::PathMetadata {
                    source: "extension:extra".to_owned().into(),
                    scope: pi_coding_agent::core::package_manager::SourceScope::Temporary,
                    origin: pi_coding_agent::core::package_manager::MetadataOrigin::TopLevel,
                    base_dir: Some(extra_prompts.to_string_lossy().into_owned().into()),
                },
            }],
            theme_paths: Vec::new(),
        })
        .unwrap();

    let skill = resource_loader
        .get_skills()
        .skills
        .into_iter()
        .find(|skill| skill.name == "extra-skill")
        .unwrap();
    assert_eq!(skill.source_info.source, "extension:extra");
    assert_eq!(
        skill.source_info.path,
        JsString::from(extra_skill.join("SKILL.md").to_str().unwrap())
    );
    let prompt = resource_loader
        .get_prompts()
        .prompts
        .into_iter()
        .find(|prompt| prompt.name == "extra")
        .unwrap();
    assert_eq!(prompt.source_info.source, "extension:extra");
    assert_eq!(
        prompt.source_info.path,
        JsString::from(extra_prompts.join("extra.md").to_str().unwrap())
    );
}

fn tool_definition(name: &str, description: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.to_owned(),
        label: name.into(),
        description: description.into(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: json!({"type": "object"}),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(|_id, _params, _signal, _on_update, _ctx| {
            Box::pin(async {
                Ok(pi_agent_core::AgentToolResult {
                    content: Vec::new(),
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        render_call: None,
        render_result: None,
    }
}

fn factory(register: impl Fn(&ExtensionApi) + Send + Sync + 'static) -> ExtensionFactory {
    Arc::new(move |api: &ExtensionApi| {
        register(api);
        Box::pin(async { Ok(()) })
    })
}

#[tokio::test]
async fn extension_conflicts_report_tools_and_flags() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();

    let first = factory(|api| {
        api.register_tool(tool_definition("duplicate-tool", "First")).unwrap();
        api.register_flag("duplicate-flag", None, FlagType::Boolean, None)
            .unwrap();
    });
    let second = factory(|api| {
        api.register_tool(tool_definition("duplicate-tool", "Second")).unwrap();
        api.register_flag("duplicate-flag", None, FlagType::Boolean, None)
            .unwrap();
    });
    let resource_loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: cwd.to_string_lossy().into_owned().into(),
        agent_dir: agent_dir.to_string_lossy().into_owned().into(),
        settings_manager: Some(settings(&cwd, &agent_dir, true)),
        event_bus: Some(Arc::new(create_event_bus())),
        extension_factories: vec![
            InlineExtension::Factory {
                extension_path: Some("<inline:first>".to_owned().into()),
                factory: first,
            },
            InlineExtension::Factory {
                extension_path: Some("<inline:second>".to_owned().into()),
                factory: second,
            },
        ],
        ..Default::default()
    })
    .unwrap();
    resource_loader.reload().await.unwrap();
    let result = resource_loader.get_extensions();
    assert_eq!(result.extensions.len(), 2);
    let messages: Vec<JsString> = result.errors.iter().map(|error| error.error.clone()).collect();
    assert_eq!(
        messages,
        vec![
            "Tool \"duplicate-tool\" conflicts with <inline:first>",
            "Flag \"--duplicate-flag\" conflicts with <inline:first>",
        ]
    );
    assert_eq!(
        result.extensions[1].read().tools["duplicate-tool"].source_info.source,
        "inline"
    );
}

#[tokio::test]
async fn disabled_inline_extension_factories_are_skipped() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    let mut values = Settings::new();
    values.insert("disabledExtensions".to_owned().into(), json!(["disabled"]).into());
    let settings_manager = Arc::new(Mutex::new(SettingsManager::in_memory(
        values,
        SettingsManagerCreateOptions {
            project_trusted: Some(true),
        },
    )));

    let resource_loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: cwd.to_string_lossy().into_owned().into(),
        agent_dir: agent_dir.to_string_lossy().into_owned().into(),
        settings_manager: Some(settings_manager),
        extension_factories: vec![
            InlineExtension::Factory {
                extension_path: Some("<first-party:disabled>".to_owned().into()),
                factory: factory(|_| {}),
            },
            InlineExtension::Factory {
                extension_path: Some("<first-party:goal>".to_owned().into()),
                factory: factory(|_| {}),
            },
        ],
        ..Default::default()
    })
    .unwrap();
    resource_loader.reload().await.unwrap();
    let paths: Vec<JsString> = resource_loader
        .get_extensions()
        .extensions
        .into_iter()
        .map(|extension| extension.read().path.clone())
        .collect();
    assert_eq!(paths, vec!["<first-party:goal>"]);
}

#[tokio::test]
async fn missing_additional_extension_path_is_reported() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    let missing = temp.path().join("missing.ts");

    let resource_loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: cwd.to_string_lossy().into_owned().into(),
        agent_dir: agent_dir.to_string_lossy().into_owned().into(),
        settings_manager: Some(settings(&cwd, &agent_dir, true)),
        additional_extension_paths: vec![missing.to_string_lossy().into_owned().into()],
        ..Default::default()
    })
    .unwrap();
    resource_loader.reload().await.unwrap();
    let errors = resource_loader.get_extensions().errors;
    assert!(
        errors
            .iter()
            .any(|error| error.error == format!("Extension path does not exist: {}", missing.to_string_lossy()))
    );
}

#[test]
fn submodule_and_missing_gitdir_climb_normally() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    std::fs::create_dir_all(&agent_dir).unwrap();
    let sup = temp.path().join("super");
    let sub = sup.join("vendor/lib");
    let sub_src = sub.join("src");
    std::fs::create_dir_all(&sub_src).unwrap();
    std::fs::write(sup.join("AGENTS.md"), "superproject instructions").unwrap();
    std::fs::write(sub.join("AGENTS.md"), "submodule instructions").unwrap();
    let sub_git_dir = sup.join(".git/modules/vendor/lib");
    std::fs::create_dir_all(&sub_git_dir).unwrap();
    std::fs::write(sub_git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(sub.join(".git"), format!("gitdir: {}\n", sub_git_dir.display())).unwrap();

    let contents: Vec<JsString> = load_project_context_files(
        &JsString::from(sub_src.to_str().unwrap()),
        &JsString::from(agent_dir.to_str().unwrap()),
    )
    .unwrap()
    .into_iter()
    .map(|file| file.content)
    .collect();
    assert_eq!(contents, vec!["superproject instructions", "submodule instructions"]);

    let corrupt = temp.path().join("corrupt");
    let corrupt_src = corrupt.join("src");
    std::fs::create_dir_all(&corrupt_src).unwrap();
    std::fs::write(corrupt.join(".git"), "gitdir: /nonexistent/path/worktrees/feat\n").unwrap();
    std::fs::write(corrupt.join("AGENTS.md"), "repo instructions").unwrap();
    std::fs::write(corrupt_src.join("AGENTS.md"), "src instructions").unwrap();
    let contents: Vec<JsString> = load_project_context_files(
        &JsString::from(corrupt_src.to_str().unwrap()),
        &JsString::from(agent_dir.to_str().unwrap()),
    )
    .unwrap()
    .into_iter()
    .map(|file| file.content)
    .collect();
    assert_eq!(contents, vec!["repo instructions", "src instructions"]);
}

#[tokio::test]
async fn no_extensions_keeps_cli_extensions() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();

    let resource_loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: cwd.to_string_lossy().into_owned().into(),
        agent_dir: agent_dir.to_string_lossy().into_owned().into(),
        settings_manager: Some(settings(&cwd, &agent_dir, true)),
        no_extensions: true,
        extension_factories: vec![InlineExtension::Named {
            name: "goal".to_owned().into(),
            factory: factory(|_| {}),
            hidden: None,
        }],
        ..Default::default()
    })
    .unwrap();
    resource_loader.reload().await.unwrap();
    assert_eq!(resource_loader.get_extensions().extensions.len(), 1);
}

#[tokio::test]
async fn default_project_trust_loader_forces_untrusted_and_loads_inline_factories() {
    let temp = TempDir::new().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    std::fs::create_dir_all(cwd.join(".pi/extensions")).unwrap();
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(
        cwd.join(".pi/extensions/project.ts"),
        "throw new Error(\"should not load\");",
    )
    .unwrap();
    let manager = Arc::new(Mutex::new(
        SettingsManager::create(
            &JsString::from(cwd.to_str().unwrap()),
            Some(&JsString::from(agent_dir.to_string_lossy().as_ref())),
            SettingsManagerCreateOptions {
                project_trusted: Some(true),
            },
        )
        .unwrap(),
    ));

    let loader = pi_coding_agent::package_manager_cli::DefaultProjectTrustExtensionsLoader::default();
    let result = loader
        .load(
            &JsString::from(cwd.to_str().unwrap()),
            &JsString::from(agent_dir.to_str().unwrap()),
            &manager,
            &[InlineExtension::Factory {
                extension_path: Some("<first-party:goal>".to_owned().into()),
                factory: factory(|_| {}),
            }],
        )
        .await
        .unwrap()
        .unwrap();

    let paths: Vec<JsString> = result
        .extensions
        .into_iter()
        .map(|extension| extension.read().path.clone())
        .collect();
    assert_eq!(paths, vec!["<first-party:goal>"]);
    assert!(!manager.lock().unwrap().is_project_trusted());
}
