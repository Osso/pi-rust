#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::model_catalog_generator::read_generator_options;
use serde_json::json;

#[test]
fn source_cli_option_precedence_and_validation_match() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for args in [
        vec![],
        vec!["--strict", "--strict", "--pretty", "--data-only"],
        vec!["--json-only", "--json-output", "/tmp/native-model-test"],
        vec!["--json-output", "/tmp/a", "--json-output", "/tmp/b"],
        vec!["--json-only"],
        vec!["--json-output"],
        vec!["--json-output", ""],
        vec!["--data-only", "--json-output", "/tmp/a"],
        vec!["--data-only", "--json-only"],
        vec!["--wat"],
        vec!["--json-output", "--pretty"],
        vec!["--help"],
    ] {
        let expected = source.request(&json!({"operation":"readGeneratorOptions","args":[args]}));
        let actual = read_generator_options(&args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>());
        match actual {
            Ok(options) => assert_eq!(
                serde_json::to_value(options).unwrap(),
                serde_json::from_str::<serde_json::Value>(expected["json"].as_str().unwrap()).unwrap(),
                "{args:?}"
            ),
            Err(error) => assert_eq!(
                error.message.to_utf8().unwrap(),
                expected["error"]["message"].as_str().unwrap(),
                "{args:?}"
            ),
        }
    }
}

#[test]
fn deleted_cwd_cli_failure_matches_original_os_error_before_fetching() {
    use std::{fs, os::fd::AsRawFd, process::Command};
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    let expected = source
        .request(&json!({"operation":"readGeneratorOptions","args":[["--json-output","relative"]],"deletedCwd":true}));
    assert_eq!(expected["error"]["code"], "ENOENT");
    assert_eq!(expected["error"]["syscall"], "uv_cwd");
    let directory = std::env::temp_dir().join(format!("pi-generator-native-deleted-cwd-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let held = fs::File::open(&directory).unwrap();
    fs::remove_dir(&directory).unwrap();
    // A held directory FD lets the real maintenance binary start in an already deleted CWD.
    // No process-global cwd mutation occurs in the parallel Rust test runner.
    let output = Command::new(env!("CARGO_BIN_EXE_generate-models"))
        .args(["--json-output", "relative"])
        .current_dir(format!("/proc/{}/fd/{}", std::process::id(), held.as_raw_fd()))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "argument error must occur before fetch logs");
    let message = format!(
        "{}: {}",
        expected["error"]["name"].as_str().unwrap(),
        expected["error"]["message"].as_str().unwrap()
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with(&message), "native {stderr:?}; source {expected}");
    for own_field in [
        format!("errno: {}", expected["error"]["errno"]),
        "code: 'ENOENT'".to_owned(),
        "syscall: 'uv_cwd'".to_owned(),
    ] {
        assert!(stderr.contains(&own_field), "missing {own_field}: {stderr}");
    }
}
