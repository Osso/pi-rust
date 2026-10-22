//! `core/auth-guidance.ts` and `core/trust-manager.ts` behavior.

#[path = "support/trust_probe.rs"]
mod trust_probe;

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::{Path, PathBuf};

use pi_coding_agent::core::auth_guidance::{
    format_no_api_key_found_message, format_no_model_selected_message, format_no_models_available_message,
    get_provider_login_help,
};
use pi_coding_agent::core::trust_manager::{
    ProjectTrustDecision, ProjectTrustStore, ProjectTrustUpdate, get_project_trust_options,
    get_project_trust_parent_path, has_trust_requiring_project_resources,
};

#[test]
#[ignore = "child probe entry point"]
fn trust_resources_probe_child() {
    trust_probe::run_child();
}

fn s(path: &Path) -> JsString {
    path.display().to_string().into()
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-trust-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn login_guidance_points_at_the_packaged_docs() {
    let docs = pi_coding_agent::config::get_docs_path();
    let expected = format!(
        "Use /login to log into a provider via OAuth or API key. See:\n  {}/providers.md\n  {}/models.md",
        docs.display(),
        docs.display()
    );
    assert_eq!(get_provider_login_help(), expected);
    assert_eq!(
        format_no_models_available_message(),
        format!("No models available. {expected}")
    );
    assert_eq!(
        format_no_model_selected_message(),
        format!("No model selected.\n\n{expected}\n\nThen use /model to select a model.")
    );
    assert_eq!(
        format_no_api_key_found_message("anthropic"),
        format!("No API key found for anthropic.\n\n{expected}")
    );
    assert_eq!(
        format_no_api_key_found_message("unknown"),
        format!("No API key found for the selected model.\n\n{expected}")
    );
}

#[test]
fn trust_decisions_are_stored_and_inherited_from_parent_directories() {
    let temp = temp_dir("inherit");
    let agent_dir = temp.join("agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let parent_dir = temp.join("trusted-parent");
    let child_dir = parent_dir.join("project");
    fs::create_dir_all(&child_dir).unwrap();

    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    assert_eq!(store.get(&s(&child_dir)).unwrap(), ProjectTrustDecision::Unknown);
    store.set(&s(&parent_dir), ProjectTrustDecision::Trusted).unwrap();
    assert_eq!(store.get(&s(&child_dir)).unwrap(), ProjectTrustDecision::Trusted);
    store.set(&s(&child_dir), ProjectTrustDecision::Untrusted).unwrap();
    assert_eq!(store.get(&s(&child_dir)).unwrap(), ProjectTrustDecision::Untrusted);
    store.set(&s(&child_dir), ProjectTrustDecision::Unknown).unwrap();
    assert_eq!(store.get(&s(&child_dir)).unwrap(), ProjectTrustDecision::Trusted);

    let entry = store.get_entry(&s(&child_dir)).unwrap().expect("parent entry applies");
    assert_eq!(
        entry.path,
        std::fs::canonicalize(&parent_dir).unwrap().display().to_string()
    );
    assert_eq!(entry.decision, ProjectTrustDecision::Trusted);
}

#[test]
fn the_trust_file_is_sorted_pretty_json_with_a_trailing_newline() {
    let temp = temp_dir("file");
    let agent_dir = temp.join("agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let first = temp.join("zulu");
    let second = temp.join("alpha");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();

    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    store
        .set_many(&[
            ProjectTrustUpdate {
                path: first.display().to_string().into(),
                decision: ProjectTrustDecision::Trusted,
            },
            ProjectTrustUpdate {
                path: second.display().to_string().into(),
                decision: ProjectTrustDecision::Untrusted,
            },
        ])
        .unwrap();

    let content = fs::read_to_string(agent_dir.join("trust.json")).unwrap();
    let first_key = std::fs::canonicalize(&first).unwrap().display().to_string();
    let second_key = std::fs::canonicalize(&second).unwrap().display().to_string();
    let expected_keys = if first_key < second_key {
        [&first_key, &second_key]
    } else {
        [&second_key, &first_key]
    };
    assert!(content.ends_with("}\n"), "content: {content}");
    let first_index = content.find(expected_keys[0]).unwrap();
    let second_index = content.find(expected_keys[1]).unwrap();
    assert!(first_index < second_index, "keys must be sorted: {content}");
    assert_eq!(content.matches("\n  \"").count(), 2, "two-space indent: {content}");
    assert!(content.contains("true"));
    assert!(content.contains("false"));
}

#[test]
fn invalid_trust_files_report_their_specific_errors() {
    let temp = temp_dir("invalid");
    let agent_dir = temp.join("agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let trust_path = agent_dir.join("trust.json");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();

    fs::write(&trust_path, "{ not json").unwrap();
    let error = store.get(&s(&temp)).unwrap_err().to_string();
    assert!(
        error.starts_with(&format!("Failed to read trust store {}", trust_path.display())),
        "error: {error:?}"
    );
    assert!(error.contains("Expected property name or '}'"), "error: {error:?}");

    fs::write(&trust_path, "[]").unwrap();
    assert_eq!(
        store.get(&s(&temp)).unwrap_err().to_string(),
        format!("Invalid trust store {}: expected an object", trust_path.display())
    );

    fs::write(&trust_path, "{\"/a\": 1}").unwrap();
    assert_eq!(
        store.get(&s(&temp)).unwrap_err().to_string(),
        format!(
            "Invalid trust store {}: value for \"/a\" must be true, false, or null",
            trust_path.display()
        )
    );

    fs::write(&trust_path, "{\"/a\": null}").unwrap();
    assert_eq!(store.get(&s(&temp)).unwrap(), ProjectTrustDecision::Unknown);
}

#[test]
fn trust_options_describe_each_choice() {
    let temp = temp_dir("options");
    let cwd = temp.join("project");
    fs::create_dir_all(&cwd).unwrap();
    let canonical_cwd = std::fs::canonicalize(&cwd).unwrap().display().to_string();
    let parent = std::fs::canonicalize(&temp).unwrap().display().to_string();
    assert_eq!(
        get_project_trust_parent_path(&JsString::from(&s(&cwd)))
            .unwrap()
            .as_ref(),
        Some(&JsString::from(parent.as_str()))
    );

    let options = get_project_trust_options(&JsString::from(&s(&cwd)), false).unwrap();
    assert_eq!(options.len(), 3);
    assert_eq!(options[0].label, "Trust");
    assert!(options[0].trusted);
    assert_eq!(
        options[0].saved_path.as_ref(),
        Some(&JsString::from(canonical_cwd.as_str()))
    );
    assert_eq!(options[1].label, format!("Trust parent folder ({parent})"));
    assert_eq!(options[1].updates.len(), 2);
    assert_eq!(options[1].updates[1].decision, ProjectTrustDecision::Unknown);
    assert_eq!(options[2].label, "Do not trust");
    assert!(!options[2].trusted);

    let options = get_project_trust_options(&JsString::from(&s(&cwd)), true).unwrap();
    assert_eq!(options.len(), 5);
    assert_eq!(options[2].label, "Trust (this session only)");
    assert!(options[2].updates.is_empty());
    assert_eq!(options[4].label, "Do not trust (this session only)");
    assert!(!options[4].trusted);
}

#[test]
fn trust_requiring_resources_are_detected_from_project_layouts() {
    let temp = temp_dir("resources");
    let project = temp.join("project");
    fs::create_dir_all(&project).unwrap();
    let home = temp.clone();
    fs::create_dir_all(home.join(".pi/agent")).unwrap();
    fs::create_dir_all(home.join(".agents/skills")).unwrap();

    let probe = |cwd: &Path| trust_probe::spawn(&s(cwd), &s(&home));
    assert!(!probe(&temp));
    assert!(!probe(&project));

    fs::create_dir_all(home.join(".codex/skills")).unwrap();
    assert!(!probe(&project));

    fs::write(home.join(".pi/settings.json"), "{}").unwrap();
    assert!(probe(&temp));
    fs::remove_file(home.join(".pi/settings.json")).unwrap();

    fs::create_dir_all(project.join(".pi")).unwrap();
    fs::write(project.join(".pi/settings.json"), "{}").unwrap();
    assert!(probe(&project));
    fs::remove_dir_all(project.join(".pi")).unwrap();

    fs::create_dir_all(project.join(".agents/skills")).unwrap();
    assert!(probe(&project));
    fs::remove_dir_all(project.join(".agents")).unwrap();

    fs::create_dir_all(project.join(".codex/skills")).unwrap();
    assert!(probe(&project));
    fs::remove_dir_all(project.join(".codex")).unwrap();

    fs::create_dir_all(project.join(".claude/skills")).unwrap();
    assert!(probe(&project));

    let workspace = temp.join("workspace");
    let nested = workspace.join("repo/package");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all(workspace.join(".codex/skills")).unwrap();
    assert!(probe(&nested));
    assert_eq!(
        has_trust_requiring_project_resources(&JsString::from(&s(&nested))).unwrap(),
        probe(&nested)
    );
}
