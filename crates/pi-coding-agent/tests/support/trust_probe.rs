//! Child-process probe for `hasTrustRequiringProjectResources`, which reads `HOME`.

use pi_js_compat::js_json::JsString;
use std::ffi::OsString;
use std::process::Command;

use pi_coding_agent::core::trust_manager::has_trust_requiring_project_resources;

pub(crate) const OUTPUT_PREFIX: &str = "PI_RUST_TRUST_OUTPUT:";
const CWD_ENV: &str = "PI_RUST_TRUST_CWD";
const MODE_ENV: &str = "PI_RUST_TRUST_MODE";

pub(crate) fn spawn(cwd: &str, home: &str) -> bool {
    let executable = std::env::current_exe().expect("trust test executable must be discoverable");
    let env: Vec<(&str, OsString)> = vec![(CWD_ENV, cwd.into()), ("HOME", home.into())];
    let mut command = Command::new(executable);
    command.env_clear().env(MODE_ENV, "resources").args([
        "--ignored",
        "--exact",
        "trust_resources_probe_child",
        "--nocapture",
        "--test-threads=1",
    ]);
    for (name, value) in env {
        command.env(name, value);
    }
    let output = command.output().expect("trust child probe must spawn");
    assert!(
        output.status.success(),
        "trust child probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("child output must be UTF-8");
    let payload = stdout
        .lines()
        .find_map(|line| {
            line.find(OUTPUT_PREFIX)
                .map(|index| line[index + OUTPUT_PREFIX.len()..].trim())
        })
        .expect("trust child must emit a prefixed value");
    payload == "true"
}

pub(crate) fn run_child() {
    let mode = std::env::var(MODE_ENV).expect("trust child mode must be set");
    assert_eq!(mode, "resources", "unknown trust probe mode {mode}");
    let cwd = std::env::var(CWD_ENV).expect("trust child cwd must be set");
    let detected = has_trust_requiring_project_resources(&JsString::from(&cwd)).unwrap();
    println!("{OUTPUT_PREFIX}{detected}");
}
