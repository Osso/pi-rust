//! Real `pi` startup with the user's settings shape: package entries and an
//! auto-discovered extension file resolve to native ports.

use std::path::Path;
use std::process::Command;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("parent dir");
    std::fs::write(path, text).expect("fixture file");
}

fn run_print(agent: &Path, temp: &Path) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_pi"))
        .args(["-p", "hi"])
        .current_dir(temp)
        .env("HOME", temp)
        .env("PI_CODING_AGENT_DIR", agent)
        .env("PI_CODING_AGENT_STATE_DIR", temp.join("state"))
        .env("PI_OFFLINE", "1")
        .env("NO_COLOR", "1")
        .output()
        .expect("pi runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8(output.stderr).expect("utf-8 stderr"),
    )
}

#[test]
fn startup_resolves_atlas_from_user_settings_package_path() {
    let temp = tempfile::tempdir().expect("temp dir");
    let agent = temp.path().join(".config/pi/agent");
    let package = temp.path().join("Repos/pi-atlas");
    write(
        &package.join("package.json"),
        r#"{"name":"@mohndoe/pi-atlas","pi":{"extensions":["./src/index.ts"]}}"#,
    );
    write(&package.join("src/index.ts"), "export default function () {}\n");
    write(
        &agent.join("settings.json"),
        r#"{"packages":["../../../Repos/pi-atlas"]}"#,
    );
    let (_, stderr) = run_print(&agent, temp.path());
    assert!(!stderr.contains("Failed to load extension"), "{stderr}");
    assert!(
        !stderr.contains("Extension does not export a valid factory function"),
        "{stderr}"
    );
}

#[test]
fn startup_resolves_native_ports_from_user_package_and_extension_paths() {
    let temp = tempfile::tempdir().expect("temp dir");
    let agent = temp.path().join("agent");
    let package = temp.path().join("repos/regex-replace-mcp");
    write(
        &package.join("package.json"),
        r#"{"name":"regex-replace-pi-tool","pi":{"extensions":["./extensions/regex-replace.ts"]}}"#,
    );
    write(
        &package.join("extensions/regex-replace.ts"),
        "export default function () {}\n",
    );
    write(
        &agent.join("extensions/disable-broadcast.ts"),
        "export default function () {}\n",
    );
    write(
        &agent.join("settings.json"),
        &format!(r#"{{"packages":["{}"]}}"#, package.display()),
    );

    let (_, stderr) = run_print(&agent, temp.path());
    assert!(!stderr.contains("Failed to load extension"), "{stderr}");

    write(&agent.join("extensions/unported.ts"), "export default function () {}\n");
    let (code, stderr) = run_print(&agent, temp.path());
    assert_eq!(code, 1, "{stderr}");
    assert!(
        stderr.contains("Extension does not export a valid factory function:") && stderr.contains("unported.ts"),
        "{stderr}"
    );
    assert!(!stderr.contains("disable-broadcast.ts"), "{stderr}");
}
