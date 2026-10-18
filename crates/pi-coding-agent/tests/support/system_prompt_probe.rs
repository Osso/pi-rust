use std::io::Read;

use pi_coding_agent::core::system_prompt::{BuildSystemPromptOptions, build_system_prompt};

pub(crate) const OUTPUT_PREFIX: &str = "PI_RUST_SYSTEM_PROMPT_OUTPUT:";

pub(crate) fn run_child_probe() {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("system prompt probe stdin must be readable");
    let options: BuildSystemPromptOptions =
        serde_json::from_str(&input).expect("system prompt options must deserialize");
    let prompt = build_system_prompt(&options);
    println!("{OUTPUT_PREFIX}{}", serde_json::to_string(&prompt).unwrap());
}

pub(crate) fn spawn_child_probe(package_dir: &str, options: &serde_json::Value) -> String {
    use std::process::{Command, Stdio};

    let executable = std::env::current_exe().expect("test executable must be discoverable");
    let mut child = Command::new(executable)
        .env_clear()
        .env("PI_PACKAGE_DIR", package_dir)
        .env("HOME", "/home/tester")
        .args([
            "--ignored",
            "--exact",
            "system_prompt_probe_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("system prompt child probe must spawn");
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("probe stdin");
        serde_json::to_writer(&mut *stdin, options).expect("probe request serializes");
        stdin.write_all(b"\n").expect("probe request newline");
    }
    let output = child.wait_with_output().expect("probe output");
    let stdout = String::from_utf8(output.stdout).expect("probe stdout is UTF-8");
    let payload = stdout
        .split(OUTPUT_PREFIX)
        .nth(1)
        .and_then(|rest| rest.split('\n').next())
        .unwrap_or_else(|| panic!("probe must print its prompt payload; stdout={stdout:?}"));
    serde_json::from_str::<String>(payload).expect("probe payload is a JSON string")
}
