use indexmap::IndexMap;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_coding_agent::core::tools::bash::{BashExecOptions, BashOperations, create_local_bash_operations};

use crate::shared_executable_script as executable_script;

#[test]
fn sends_commands_over_stdin_for_the_legacy_wsl_shim() {
    let temp = tempfile::tempdir().expect("temp dir");
    let shell_relative = "C:\\Windows\\System32\\bash.exe";
    let shell_path = temp.path().join(shell_relative);
    std::fs::create_dir_all(shell_path.parent().expect("parent")).expect("parent dirs");
    executable_script::write_executable_script(&shell_path, "#!/bin/sh\ncat\n");

    let original_cwd = std::env::current_dir().expect("cwd");
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(temp.path()),
    )))
    .expect("chdir");
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let chunks = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = Arc::clone(&chunks);
    let operations = create_local_bash_operations(Some(&JsString::from(shell_relative)));
    let command = "name='World'; echo \"Hello, ${name}!\"";
    let mut environment = std::env::vars()
        .map(|(key, value)| (key.into(), Some(value.into())))
        .collect::<IndexMap<JsString, Option<JsString>>>();
    environment.insert(
        "PATH".into(),
        Some(format!("{}:{}", temp.path().display(), std::env::var("PATH").unwrap()).into()),
    );
    let result = runtime
        .block_on(operations.exec(
            &JsString::from(command),
            &JsString::from(temp.path().to_str().expect("utf-8 test path")),
            BashExecOptions {
                on_data: Arc::new(move |data| {
                    captured.lock().expect("chunks").extend_from_slice(data);
                    Ok(())
                }),
                signal: None,
                timeout: None,
                env: Some(environment),
                detach: None,
            },
        ))
        .expect("stdin shell");
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(original_cwd),
    )))
    .expect("restore cwd");
    assert_eq!(result.exit_code, Some(0.0));
    let output = String::from_utf8(chunks.lock().expect("chunks").clone()).expect("utf8");
    assert_eq!(output, command);
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
