//! `config.ts` path and constant behavior under controlled environments. Every scenario runs in a
//! child process because the TypeScript module reads the environment per call and the package
//! constants once per process.

#[path = "support/config_probe.rs"]
mod config_probe;

use std::fs;
use std::path::{Path, PathBuf};

use config_probe::{paths_payload, spawn_child_probe};
use serde_json::Value;

#[test]
#[ignore = "child probe entry point"]
fn config_probe_child() {
    config_probe::run_child_probe();
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pi-config-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_package(dir: &Path, content: &str) {
    fs::write(dir.join("package.json"), content).unwrap();
}

fn probe(env: &[(&str, String)]) -> Value {
    let pairs: Vec<(&str, std::ffi::OsString)> = env.iter().map(|(name, value)| (*name, value.into())).collect();
    spawn_child_probe("paths", &pairs)
}

#[test]
fn xdg_roots_and_explicit_overrides_follow_the_typescript_rules() {
    let home = temp_dir("home-xdg");
    let payload = probe(&[
        ("HOME", home.display().to_string()),
        ("XDG_CONFIG_HOME", "/tmp/pi-xdg-config".to_owned()),
        ("XDG_STATE_HOME", "/tmp/pi-xdg-state".to_owned()),
        ("PI_PACKAGE_DIR", home.display().to_string()),
    ]);
    assert_eq!(payload["userConfigRoot"], "/tmp/pi-xdg-config/pi");
    assert_eq!(payload["agentDir"], "/tmp/pi-xdg-config/pi/agent");
    assert_eq!(payload["userStateRoot"], "/tmp/pi-xdg-state/pi");
    assert_eq!(payload["legacyAgentDir"], format!("{}/.pi/agent", home.display()));
    assert_eq!(payload["authPath"], "/tmp/pi-xdg-config/pi/agent/auth.json");
    assert_eq!(payload["modelsPath"], "/tmp/pi-xdg-config/pi/agent/models.json");
    assert_eq!(payload["settingsPath"], "/tmp/pi-xdg-config/pi/agent/settings.json");
    assert_eq!(payload["binDir"], "/tmp/pi-xdg-config/pi/agent/bin");
    assert_eq!(payload["toolsDir"], "/tmp/pi-xdg-config/pi/agent/tools");
    assert_eq!(payload["promptsDir"], "/tmp/pi-xdg-config/pi/agent/prompts");
    assert_eq!(
        payload["promptHistoryPath"],
        "/tmp/pi-xdg-config/pi/agent/prompt-history.json"
    );
    assert_eq!(payload["sessionsDir"], "/tmp/pi-xdg-config/pi/agent/sessions");
    assert_eq!(payload["customThemesDir"], "/tmp/pi-xdg-config/pi/agent/themes");
    assert_eq!(payload["debugLogPath"], "/tmp/pi-xdg-config/pi/agent/pi-debug.log");
    assert_eq!(payload["envAgentDir"], "PI_CODING_AGENT_DIR");
    assert_eq!(payload["envSessionDir"], "PI_CODING_AGENT_SESSION_DIR");
    assert_eq!(payload["envStateDir"], "PI_CODING_AGENT_STATE_DIR");
    assert_eq!(payload["shareViewerUrl"], "https://pi.dev/session/#abc123");
}

#[test]
fn home_fallbacks_and_tilde_overrides_apply() {
    let home = temp_dir("home-fallback");
    let package = temp_dir("package-fallback");
    write_package(
        &package,
        "{\"name\":\"@earendil-works/pi-coding-agent\",\"version\":\"1.2.3\"}",
    );
    let payload = probe(&[
        ("HOME", home.display().to_string()),
        ("PI_PACKAGE_DIR", package.display().to_string()),
    ]);
    assert_eq!(payload["userConfigRoot"], format!("{}/.config/pi", home.display()));
    assert_eq!(payload["userStateRoot"], format!("{}/.local/state/pi", home.display()));
    assert_eq!(payload["expandedTilde"], format!("{}/nested/dir", home.display()));

    let payload = probe(&[
        ("HOME", home.display().to_string()),
        ("PI_CODING_AGENT_DIR", "~/custom-agent".to_owned()),
        ("PI_CODING_AGENT_STATE_DIR", "~/custom-state".to_owned()),
        ("XDG_CONFIG_HOME", "/tmp/ignored".to_owned()),
        ("XDG_STATE_HOME", "/tmp/ignored".to_owned()),
        ("PI_PACKAGE_DIR", package.display().to_string()),
    ]);
    assert_eq!(payload["agentDir"], format!("{}/custom-agent", home.display()));
    assert_eq!(payload["userStateRoot"], format!("{}/custom-state", home.display()));
    assert_eq!(
        payload["authPath"],
        format!("{}/custom-agent/auth.json", home.display())
    );
}

#[test]
fn package_assets_resolve_from_the_package_dir() {
    let package = temp_dir("package-assets");
    write_package(
        &package,
        "{\"name\":\"@earendil-works/pi-coding-agent\",\"version\":\"1.2.3\"}",
    );
    let payload = probe(&[
        ("HOME", "/home/tester".to_owned()),
        ("PI_PACKAGE_DIR", package.display().to_string()),
    ]);
    let base = package.display();
    assert_eq!(payload["packageDir"], base.to_string());
    assert_eq!(payload["packageJsonPath"], format!("{base}/package.json"));
    assert_eq!(payload["readmePath"], format!("{base}/README.md"));
    assert_eq!(payload["docsPath"], format!("{base}/docs"));
    assert_eq!(payload["examplesPath"], format!("{base}/examples"));
    assert_eq!(payload["changelogPath"], format!("{base}/CHANGELOG.md"));
    // Compiled-binary layout: assets ship next to the executable, not under src/ or dist/.
    assert_eq!(payload["themesDir"], format!("{base}/theme"));
    assert_eq!(payload["exportTemplateDir"], format!("{base}/export-html"));
    assert_eq!(payload["interactiveAssetsDir"], format!("{base}/assets"));
    assert_eq!(payload["bundledAsset"], format!("{base}/assets/logo.txt"));
}

#[test]
fn package_constants_come_from_package_json_piconfig() {
    let package = temp_dir("package-config");
    write_package(
        &package,
        "{\"name\":\"@acme/agent\",\"version\":\"9.9.9\",\"piConfig\":{\"name\":\"tau\",\"configDir\":\".tau\"}}",
    );
    let payload = probe(&[
        ("HOME", "/home/tester".to_owned()),
        ("PI_PACKAGE_DIR", package.display().to_string()),
    ]);
    assert_eq!(payload["packageName"], "@acme/agent");
    assert_eq!(payload["version"], "9.9.9");
    assert_eq!(payload["appName"], "tau");
    assert_eq!(payload["appTitle"], "tau");
    assert_eq!(payload["configDirName"], ".tau");
    assert_eq!(payload["envAgentDir"], "TAU_CODING_AGENT_DIR");
    assert_eq!(payload["legacyAgentDir"], "/home/tester/.tau/agent");
    assert_eq!(payload["userConfigRoot"], "/home/tester/.config/tau");
    assert_eq!(payload["debugLogPath"], "/home/tester/.config/tau/agent/tau-debug.log");
}

#[test]
fn missing_package_json_uses_the_default_constants() {
    let package = temp_dir("package-missing");
    let payload = probe(&[
        ("HOME", "/home/tester".to_owned()),
        ("PI_PACKAGE_DIR", package.display().to_string()),
    ]);
    assert_eq!(payload["packageName"], "@earendil-works/pi-coding-agent");
    assert_eq!(payload["version"], "0.0.0");
    assert_eq!(payload["appName"], "pi");
    assert_eq!(payload["appTitle"], "π");
    assert_eq!(payload["configDirName"], ".pi");
    assert_eq!(payload["shareViewerUrl"], "https://pi.dev/session/#abc123");

    let payload = probe(&[
        ("HOME", "/home/tester".to_owned()),
        ("PI_PACKAGE_DIR", package.display().to_string()),
        ("PI_SHARE_VIEWER_URL", "https://example.test/s/".to_owned()),
    ]);
    assert_eq!(payload["shareViewerUrl"], "https://example.test/s/#abc123");
}

#[test]
fn invalid_package_json_reports_the_parse_error() {
    let package = temp_dir("package-invalid");
    write_package(&package, "{ not json");
    let error = pi_coding_agent::config::PackageConfig::load(&package).expect_err("invalid package.json must fail");
    assert!(
        error.to_string().contains("Expected property name or '}'"),
        "unexpected error: {error}"
    );
}

#[test]
fn default_payload_matches_this_process() {
    // The direct payload helper is what the child prints; keep it covered in-process too.
    let payload = paths_payload();
    assert!(payload["agentDir"].is_string());
}
