#[path = "../examples/extensions/tool_override.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::json;
use std::sync::Arc;
#[tokio::test]
async fn audits_reads_and_blocks_sensitive_paths_with_complete_results() {
    let temp = tempfile::tempdir().unwrap();
    let agent = temp.path().join("agent");
    std::fs::create_dir(&agent).unwrap();
    pi_js_compat::process_env::set("PI_CODING_AGENT_DIR", agent.to_str().unwrap());
    let millis = 1_700_000_000_000_i64;
    let default: ExtensionFactory = Arc::new(|api| {
        let result = example::tool_override_extension(api);
        Box::pin(async move { result })
    });
    support::compare("tool-override",default,json!({"agentDir":agent,"cases":[{"event":"tool_metadata","toolName":"read"},{"event":"command","name":"read-log"}]})).await;
    std::fs::write(temp.path().join("file.txt"), "one\ntwo\nλ🚀\n").unwrap();
    std::fs::write(temp.path().join("large.txt"), "λ".repeat(11000)).unwrap();
    std::fs::write(temp.path().join("invalid.txt"), [0xff, 0xfe, b'A']).unwrap();
    let mut cases = vec![json!({"event":"tool_metadata","toolName":"read"})];
    for path in [
        ".env",
        "folder/.env.prod",
        "secret.json",
        "SECRETS.YAML",
        "credentials.toml",
        "x/.ssh/id",
        "x/.aws/config",
        "x/.gnupg/key",
        ".envx",
        "missing",
        "file.txt",
        ".",
        "large.txt",
        "invalid.txt",
    ] {
        cases.push(json!({"event":"execute","toolName":"read","params":{"path":path}}));
    }
    cases.push(json!({"event":"execute","toolName":"read","params":{"path":format!("file://{}",temp.path().join("file.txt").display())}}));
    for (offset, limit) in [
        (1.0, 2.0),
        (2.0, 1.0),
        (0.0, 0.0),
        (-2.0, -1.0),
        (1.5, 1.2),
        (100.0, 2.0),
    ] {
        cases.push(
            json!({"event":"execute","toolName":"read","params":{"path":"file.txt","offset":offset,"limit":limit}}),
        );
    }
    cases.push(json!({"event":"command","name":"read-log"}));
    let factory: ExtensionFactory = Arc::new(move |api| {
        let result = example::register_with_clock(api, Arc::new(move || millis));
        Box::pin(async move { result })
    });
    let actual=support::compare("tool-override",factory,json!({"cwd":temp.path(),"agentDir":agent,"nowMillis":millis,"resetFiles":[agent.join("read-access.log")],"cases":cases})).await;
    assert_eq!(
        actual["cases"][1]["result"]["value"]["details"],
        json!({"blocked":true})
    );
    assert_eq!(
        std::fs::read_to_string(agent.join("read-access.log"))
            .unwrap()
            .lines()
            .count(),
        21
    );
    let missing_agent = temp.path().join("missing-agent");
    pi_js_compat::process_env::set("PI_CODING_AGENT_DIR", missing_agent.to_str().unwrap());
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::tool_override_extension(api);
        Box::pin(async move { result })
    });
    support::compare(
        "tool-override",
        factory,
        json!({"cwd":temp.path(),"agentDir":missing_agent,"cases":[
            {"event":"execute","toolName":"read","params":{"path":"file.txt"}},
            {"event":"execute","toolName":"read","params":{"path":".env"}},
            {"event":"command","name":"read-log"}
        ]}),
    )
    .await;
    assert!(!missing_agent.exists());
    pi_js_compat::process_env::clear();
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
