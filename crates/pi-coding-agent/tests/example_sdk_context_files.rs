#[path = "../examples/sdk/07_context_files.rs"]
mod example;
use crate::shared_oracle as oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use serde_json::json;

#[tokio::test]
async fn merges_context_and_preserves_utf16_lengths_and_source_count_bug() {
    let fixture = support::fixture().await;
    let path = fixture.home.path().join("project/AGENTS.md");
    std::fs::write(&path, "# Local 😀\n").unwrap();
    let mut real = oracle::TsOracle::spawn(
        "sdk-resources-state-oracle.ts",
        "packages/coding-agent/examples/sdk/07-context-files.ts",
    );
    let state = real.request(&json!({"cwd":fixture.options.cwd,"agentDir":fixture.options.agent_dir}));
    let output = support::Output::default();
    let _main = example::main;
    let (files, session) = example::run(fixture.options, output.clone()).await.unwrap();
    assert_eq!(session.resource_loader().get_agents_files().agents_files, files);
    assert!(session.system_prompt().contains("# Local 😀"));
    assert!(session.system_prompt().contains("- Prefer const over let"));
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/07-context-files.ts",
    );
    let expected = source
        .request(&json!({"agents":{"agentsFiles":[{"path":path.display().to_string(),"content":"# Local 😀\n"}]}}));
    assert_eq!(output.text(), expected["stdout"]);
    assert_eq!(serde_json::to_value(&files).unwrap(), state["agents"]);
    assert!(
        state["systemPrompt"]
            .as_str()
            .unwrap()
            .contains("- Prefer const over let")
    );
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].content, "# Local 😀\n");
    assert!(files[1].content.contains("- Prefer const over let"));
    assert!(output.text().ends_with("Session created with 3 context files\n"));
    assert!(fixture.requests.lock().unwrap().is_empty());
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
