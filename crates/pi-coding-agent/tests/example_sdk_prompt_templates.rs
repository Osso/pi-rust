#[path = "../examples/sdk/08_prompt_templates.rs"]
mod example;
use crate::shared_oracle as oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use serde_json::json;

#[tokio::test]
async fn appends_deploy_template_and_preserves_source_count_bug() {
    let fixture = support::fixture().await;
    let path = fixture.home.path().join("project/.pi/prompts");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("hello.md"), "---\ndescription: Greet the user\n---\nHello $1").unwrap();
    let mut real = oracle::TsOracle::spawn(
        "sdk-resources-state-oracle.ts",
        "packages/coding-agent/examples/sdk/08-prompt-templates.ts",
    );
    let state = real.request(&json!({"cwd":fixture.options.cwd,"agentDir":fixture.options.agent_dir}));
    let output = support::Output::default();
    let _main = example::main;
    let (prompts, session) = example::run(fixture.options, output.clone()).await.unwrap();
    assert_eq!(session.prompt_templates(), prompts);
    assert_eq!(session.resource_loader().get_prompts().prompts, prompts);
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/08-prompt-templates.ts",
    );
    let expected = source
        .request(&json!({"prompts":{"prompts":[{"name":"hello","description":"Greet the user"}],"diagnostics":[]}}));
    assert_eq!(output.text(), expected["stdout"]);
    assert_eq!(serde_json::to_value(&prompts).unwrap(), state["prompts"]);
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[1].name, "deploy");
    assert_eq!(prompts[1].source_info.source, "sdk");
    assert_eq!(
        prompts[1].content,
        "# Deploy Instructions\n\n1. Build: npm run build\n2. Test: npm test\n3. Deploy: npm run deploy"
    );
    assert!(output.text().ends_with("Session created with 3 prompt templates\n"));
    assert!(fixture.requests.lock().unwrap().is_empty());
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
