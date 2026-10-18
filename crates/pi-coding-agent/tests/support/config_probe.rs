use std::ffi::OsString;
use std::process::{Command, Output};

use serde_json::{Value, json};

pub(crate) const OUTPUT_PREFIX: &str = "PI_RUST_CONFIG_OUTPUT:";
const MODE_ENV: &str = "PI_RUST_CONFIG_PROBE_MODE";

pub(crate) fn paths_payload() -> Value {
    use pi_coding_agent::config;
    json!({
        "packageDir": config::get_package_dir(),
        "packageName": config::package_config().package_name,
        "appName": config::package_config().app_name,
        "appTitle": config::package_config().app_title,
        "configDirName": config::package_config().config_dir_name,
        "version": config::package_config().version,
        "envAgentDir": config::env_agent_dir(),
        "envSessionDir": config::env_session_dir(),
        "envStateDir": config::env_state_dir(),
        "userConfigRoot": config::get_user_config_root(),
        "userStateRoot": config::get_user_state_root(),
        "legacyAgentDir": config::get_legacy_agent_dir(),
        "agentDir": config::get_agent_dir(),
        "customThemesDir": config::get_custom_themes_dir(),
        "modelsPath": config::get_models_path(),
        "authPath": config::get_auth_path(),
        "settingsPath": config::get_settings_path(),
        "toolsDir": config::get_tools_dir(),
        "binDir": config::get_bin_dir(),
        "promptsDir": config::get_prompts_dir(),
        "promptHistoryPath": config::get_prompt_history_path(),
        "sessionsDir": config::get_sessions_dir(),
        "debugLogPath": config::get_debug_log_path(),
        "themesDir": config::get_themes_dir(),
        "exportTemplateDir": config::get_export_template_dir(),
        "packageJsonPath": config::get_package_json_path(),
        "readmePath": config::get_readme_path(),
        "docsPath": config::get_docs_path(),
        "examplesPath": config::get_examples_path(),
        "changelogPath": config::get_changelog_path(),
        "interactiveAssetsDir": config::get_interactive_assets_dir(),
        "bundledAsset": config::get_bundled_interactive_asset_path("logo.txt"),
        "shareViewerUrl": config::get_share_viewer_url("abc123"),
        "expandedTilde": config::expand_tilde_path("~/nested/dir"),
    })
}

pub(crate) fn spawn_child_probe(mode: &str, env_pairs: &[(&str, OsString)]) -> Value {
    let output = child_probe_command(mode, env_pairs)
        .output()
        .expect("config child probe must spawn");
    parse_child_probe_output(output)
}

fn child_probe_command(mode: &str, env_pairs: &[(&str, OsString)]) -> Command {
    let executable = std::env::current_exe().expect("config test executable must be discoverable");
    let mut command = Command::new(executable);
    command.env_clear().env(MODE_ENV, mode).args([
        "--ignored",
        "--exact",
        "config_probe_child",
        "--nocapture",
        "--test-threads=1",
    ]);
    for (name, value) in env_pairs {
        command.env(name, value);
    }
    command
}

fn parse_child_probe_output(output: Output) -> Value {
    assert!(
        output.status.success(),
        "config child probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("config child output must be UTF-8");
    let payloads = stdout
        .lines()
        .filter_map(|line| {
            line.find(OUTPUT_PREFIX)
                .map(|index| &line[index + OUTPUT_PREFIX.len()..])
        })
        .collect::<Vec<_>>();
    assert_eq!(payloads.len(), 1, "config child must emit one prefixed JSON value");
    serde_json::from_str(payloads[0]).expect("config child prefixed output must be JSON")
}

pub(crate) fn run_child_probe() {
    let mode = std::env::var(MODE_ENV).expect("config child mode must be set");
    let payload = match mode.as_str() {
        "paths" => paths_payload(),
        other => panic!("unknown config probe mode {other}"),
    };
    println!("{OUTPUT_PREFIX}{}", serde_json::to_string(&payload).unwrap());
}
