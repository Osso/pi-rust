#[path = "../examples/extensions/bash_spawn_hook.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::json;
use std::sync::Arc;
#[tokio::test]
async fn registered_bash_sources_only_private_profile_preserves_cwd_and_sets_env() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let cwd = temp.path().join("project");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&cwd).unwrap();
    std::fs::write(home.join(".profile"), "export PROFILE_PROBE='private-profile'\n").unwrap();
    pi_js_compat::process_env::set("HOME", home.to_str().unwrap());
    let previous = std::env::current_dir().unwrap();
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(&(&cwd)))).unwrap();
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::bash_spawn_hook_extension(api);
        Box::pin(async move { result })
    });
    let actual = support::compare(
        "bash-spawn-hook",
        factory,
        json!({"cwd":cwd,"home":home,"cases":[
         {"event":"tool_metadata"},
         {"event":"execute","params":{"command":"printf '%s|%s|%s' \"$PI_SPAWN_HOOK\" \"$PROFILE_PROBE\" \"$PWD\""}},
         {"event":"execute","params":{"command":"printf 'failed-private'; exit 7"}}
        ]}),
    )
    .await;
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(previous),
    )))
    .unwrap();
    pi_js_compat::process_env::clear();
    assert_eq!(actual["metadata"], json!({"commands":[],"tools":["bash"]}));
    assert_eq!(
        actual["cases"][1]["result"]["value"]["content"][0]["text"],
        format!("1|private-profile|{}", cwd.display())
    );
    assert!(actual["cases"][2]["result"]["error"].as_str().unwrap().contains("7"));
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
#[path = "support/ui_thread.rs"]
pub mod shared_ui_thread;
