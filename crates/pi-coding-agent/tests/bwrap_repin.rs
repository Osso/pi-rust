//! Frozen bwrap registration/status behavior without real sandboxes.
#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/extensions.rs"]
mod support;
use pi_coding_agent::bundled_extensions::bwrap::{
    backend::create_sandboxed_bash_operations,
    index::{BwrapExtensionOptions, install_bwrap_extension},
};
use pi_coding_agent::core::{
    extensions::{create_extension_runtime, types::NotifyType},
    settings_manager::Settings,
    tools::bash::BashExecOptions,
};
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn status_command_reports_unavailable_sandbox_but_not_full_access() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing-bwrap").display().to_string();
    for (profile, expected_type, mode) in [
        (
            "read-only",
            NotifyType::Error,
            format!(
                "read-only: bubblewrap is required for sandbox profile but is unavailable: spawnSync {missing} ENOENT; tools blocked"
            ),
        ),
        ("full-access", NotifyType::Info, "unsandboxed/full-access".to_owned()),
    ] {
        let runtime = create_extension_runtime();
        let command = missing.clone();
        let extension = support::extension(&runtime, "/repo", "<bwrap-test>", move |api| {
            install_bwrap_extension(
                api,
                BwrapExtensionOptions {
                    bwrap_command: Some(command.into()),
                },
            )
            .unwrap()
        })
        .await;
        let mut options = bundled::HarnessOptions::new("/repo");
        options.settings =
            Some(serde_json::from_value::<Settings>(serde_json::json!({"sandboxProfile":profile})).unwrap());
        let handler = extension.read().commands["bwrap"].handler.clone();
        let harness = bundled::Harness::new(vec![extension], runtime, options).await;
        handler("".into(), harness.command_context()).await.unwrap();
        assert_eq!(
            harness.ui.last_notification(),
            Some((
                format!("bwrap backend: {mode}\ncwd: /repo\nbwrap: {missing}").into(),
                Some(expected_type)
            ))
        );
    }
}

#[tokio::test]
async fn full_access_preserves_configured_shell() {
    let directory = tempfile::tempdir().unwrap();
    let shell = directory.path().join("shell.sh");
    let body = "#!/bin/sh\nprintf 'custom-shell:%s' \"$*\"\n";
    executable_script::write_executable_script(&shell, body);
    let operations =
        create_sandboxed_bash_operations("never-run-bwrap".into(), None, Some(shell.to_str().unwrap().into()));
    let output = Arc::new(Mutex::new(Vec::new()));
    let recorded = output.clone();
    let result = operations
        .exec(
            &JsString::from("echo hi"),
            &directory.path().to_str().unwrap().into(),
            BashExecOptions {
                on_data: Arc::new(move |bytes| {
                    recorded.lock().unwrap().extend_from_slice(bytes);
                    Ok(())
                }),
                signal: None,
                timeout: None,
                env: None,
                detach: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(result.exit_code, Some(0.0));
    assert!(
        String::from_utf8(output.lock().unwrap().clone())
            .unwrap()
            .starts_with("custom-shell:")
    );
}

#[path = "support/executable_script.rs"]
mod executable_script;
