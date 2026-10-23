use std::{path::Path, process::Command};

#[test]
fn runtime_construction_retains_native_configuration_errors() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("passwd-failure.so");
    let output = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/passwd_failure.c"))
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "runtime_error_child", "--nocapture"])
        .env("LD_PRELOAD", library)
        .env("PI_TEST_FAIL_PASSWD", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

fn assert_system_error(error: pi_ai::request::CallbackError) {
    let pi_js_compat::abort::AbortReason::Error(native) = error.thrown_value() else {
        panic!("configuration rejection must remain an Error");
    };
    assert_eq!(native.constructor_class().name(), "SystemError");
    assert_eq!(native.value()["info"]["code"], pi_js_compat::js_json!("EIO"));
    assert!(error.original.is_some(), "native source must survive the carrier");
}

#[tokio::test]
#[ignore = "isolated forced-NSS construction child"]
async fn runtime_error_child() {
    for name in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        &pi_coding_agent::config::env_agent_dir(),
        &pi_coding_agent::config::env_state_dir(),
    ] {
        pi_js_compat::process_env::remove(name);
    }
    let error = match pi_coding_agent::core::sdk::create_agent_session(Default::default()).await {
        Ok(_) => panic!("SDK must reject home lookup failure"),
        Err(error) => error,
    };
    assert_system_error(error);
}
