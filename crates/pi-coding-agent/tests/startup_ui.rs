//! `cli/startup-ui.ts` predicates and theme loading.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pi_coding_agent::cli::startup_ui::{
    is_official_distribution, load_startup_themes, load_themes, should_run_first_time_setup,
};
use pi_coding_agent::config;
use pi_coding_agent::core::package_manager::ResolvedResource;
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use serde_json::json;

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-startup-ui-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn dark_theme_json() -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json")).unwrap())
        .unwrap()
}

fn write_theme(dir: &Path, file: &str, name: &str) -> String {
    let mut theme = dark_theme_json();
    theme["name"] = json!(name);
    let path = dir.join(file);
    fs::write(&path, serde_json::to_string(&theme).unwrap()).unwrap();
    path.display().to_string()
}

fn resource(path: &str, enabled: bool) -> ResolvedResource {
    ResolvedResource {
        path: path.to_owned(),
        enabled,
        metadata: pi_coding_agent::core::package_manager::PathMetadata {
            source: "test".to_owned(),
            scope: pi_coding_agent::core::package_manager::SourceScope::User,
            origin: pi_coding_agent::core::package_manager::MetadataOrigin::TopLevel,
            base_dir: None,
        },
    }
}

#[test]
fn official_distribution_requires_all_three_identifiers() {
    assert!(is_official_distribution("@earendil-works/pi-coding-agent", "pi", ".pi"));
    assert!(!is_official_distribution("other", "pi", ".pi"));
    assert!(!is_official_distribution(
        "@earendil-works/pi-coding-agent",
        "fork",
        ".pi"
    ));
    assert!(!is_official_distribution(
        "@earendil-works/pi-coding-agent",
        "pi",
        ".fork"
    ));
}

#[test]
fn first_time_setup_needs_official_distribution_experimental_flag_default_agent_dir_and_missing_settings() {
    let dir = temp_dir("first-time");
    let settings = dir.join("settings.json");
    let agent_env = config::env_agent_dir();

    assert!(should_run_first_time_setup(
        &settings,
        &env(&[("PI_EXPERIMENTAL", "1")])
    ));

    assert!(!should_run_first_time_setup(
        &settings,
        &env(&[("PI_EXPERIMENTAL", "0")])
    ));
    assert!(!should_run_first_time_setup(
        &settings,
        &env(&[("PI_EXPERIMENTAL", "1"), (agent_env.as_str(), "/tmp/agent")])
    ));

    fs::write(&settings, "{}").unwrap();
    assert!(!should_run_first_time_setup(
        &settings,
        &env(&[("PI_EXPERIMENTAL", "1")])
    ));
}

#[test]
fn load_themes_skips_disabled_and_broken_entries_and_dedupes_names() {
    let dir = temp_dir("themes");
    let first = write_theme(&dir, "first.json", "custom");
    let duplicate = write_theme(&dir, "duplicate.json", "custom");
    let second = write_theme(&dir, "second.json", "other");
    let broken = dir.join("broken.json");
    fs::write(&broken, "{ not json").unwrap();

    let themes = load_themes(&[
        resource(&first, true),
        resource(&duplicate, true),
        resource(&second, true),
        resource(&broken.display().to_string(), true),
        resource(&first, false),
        resource(&dir.join("missing.json").display().to_string(), true),
    ]);
    let names: Vec<&str> = themes.iter().filter_map(|theme| theme.name.as_deref()).collect();
    assert_eq!(names, vec!["custom", "other"]);
}

#[tokio::test]
async fn load_startup_themes_uses_global_settings_only() {
    let dir = temp_dir("startup-themes");
    let agent_dir = dir.join("agent");
    fs::create_dir_all(agent_dir.join("themes")).unwrap();
    write_theme(&agent_dir.join("themes"), "custom.json", "startup");

    let settings_manager = SettingsManager::in_memory(
        json!({ "themes": [agent_dir.join("themes/custom.json").display().to_string()] })
            .as_object()
            .unwrap()
            .clone(),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    );
    let themes = load_startup_themes(&settings_manager).await.unwrap();
    let names: Vec<&str> = themes.iter().filter_map(|theme| theme.name.as_deref()).collect();
    assert!(names.contains(&"startup"), "{names:?}");
}
