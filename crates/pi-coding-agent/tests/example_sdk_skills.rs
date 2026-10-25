#[path = "../examples/sdk/04_skills.rs"]
mod example;
use crate::shared_oracle as oracle;
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
    // Frozen package-manager.ts:362 consumes readdir order, not a name sort.
    // Compare complete records without requiring Node/libuv enumeration order.
    let mut actual_skills = serde_json::to_value(&skills).unwrap().as_array().unwrap().clone();
    let mut source_skills = state["skills"].as_array().unwrap().clone();
    actual_skills.sort_by_key(|skill| skill["name"].as_str().unwrap().to_owned());
    source_skills.sort_by_key(|skill| skill["name"].as_str().unwrap().to_owned());
    assert_eq!(actual_skills, source_skills);
    assert_eq!(skills.len(), 3);
    assert_eq!(
        skills.last().unwrap().name,
        "my-skill",
        "the TS callback appends the inline skill last"
    );
    assert!(state["systemPrompt"].as_str().unwrap().contains("my-skill"));
    assert!(session.system_prompt().contains("my-skill"));
    assert!(!session.system_prompt().contains("<name>other</name>"));
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/04-skills.ts",
    );
    let mut discovery: Vec<_> = skills.iter().map(|skill| json!({"name":skill.name})).collect();
    discovery.push(json!({"name":"other"}));
    // The program/console proof gets identical explicit resource order; no output is rewritten.
    let expected = source.request(&json!({"skills":{"skills":discovery,"diagnostics":[]}}));
    assert_eq!(output.text(), expected["stdout"]);
    assert!(output.text().ends_with("Session created with filtered skills\n"));
    assert!(fixture.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn warning_diagnostics_use_literal_node_object_inspection() {
    let fixture = support::fixture().await;
    for (name, content) in [
        (
            "browser-local",
            "---\nname: browser-local\ndescription: fixture\n---\nInstructions\n",
        ),
        (
            "missing-description",
            "---\nname: missing-description\n---\nInstructions\n",
        ),
    ] {
        let path = fixture.home.path().join("agent/skills").join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("SKILL.md"), content).unwrap();
    }
    let mut state_source = oracle::TsOracle::spawn(
        "sdk-resources-state-oracle.ts",
        "packages/coding-agent/examples/sdk/04-skills.ts",
    );
    let state = state_source.request(&json!({"cwd":fixture.options.cwd,"agentDir":fixture.options.agent_dir}));
    assert!(
        !state["diagnostics"].as_array().unwrap().is_empty(),
        "fixture must exercise warning rendering"
    );
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/04-skills.ts",
    );
    let expected = source.request(&json!({"skills":{"skills":state["skills"],"diagnostics":state["diagnostics"]}}));
    let output = support::Output::default();
    let session = example::run(fixture.options, output.clone()).await.unwrap();
    assert_eq!(
        serde_json::to_value(&session.resource_loader().get_skills().diagnostics).unwrap(),
        state["diagnostics"]
    );
    assert_eq!(output.text(), expected["stdout"]);
    assert!(fixture.requests.lock().unwrap().is_empty());
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
