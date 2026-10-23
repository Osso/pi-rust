#[path = "../examples/sdk/04_skills.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use serde_json::json;

#[tokio::test]
async fn filters_discovered_skills_and_appends_virtual_skill() {
    let fixture = support::fixture().await;
    for name in ["browser-local", "other", "search-local"] {
        let path = fixture.home.path().join("agent/skills").join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: fixture\n---\nInstructions\n"),
        )
        .unwrap();
    }
    let mut real = oracle::TsOracle::spawn(
        "sdk-resources-state-oracle.ts",
        "packages/coding-agent/examples/sdk/04-skills.ts",
    );
    let state = real.request(&json!({"cwd":fixture.options.cwd,"agentDir":fixture.options.agent_dir}));
    let output = support::Output::default();
    let _main = example::main;
    let session = example::run(fixture.options, output.clone()).await.unwrap();
    let skills = session.resource_loader().get_skills().skills;
    assert_eq!(serde_json::to_value(&skills).unwrap(), state["skills"]);
    assert_eq!(
        skills.iter().map(|skill| skill.name.clone()).collect::<Vec<_>>(),
        ["browser-local", "search-local", "my-skill"]
    );
    assert!(state["systemPrompt"].as_str().unwrap().contains("my-skill"));
    assert!(session.system_prompt().contains("my-skill"));
    assert!(!session.system_prompt().contains("<name>other</name>"));
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/04-skills.ts",
    );
    let expected = source.request(&json!({"skills":{"skills":[{"name":"browser-local"},{"name":"other"},{"name":"search-local"}],"diagnostics":[]}}));
    assert_eq!(output.text(), expected["stdout"]);
    assert_eq!(
        output.text(),
        "Discovered skills: [ 'browser-local', 'search-local', 'my-skill' ]\nSession created with filtered skills\n"
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
}
