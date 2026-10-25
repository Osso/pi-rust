#[path = "../examples/extensions/prompt_customizer.rs"]
mod example;
use crate::shared_extension_examples_e as harness;
use serde_json::json;

#[tokio::test]
async fn complete_prompt_results_match_all_tool_skill_and_append_branches() {
    let skill = |name| json!({"name":name,"description":"fixture","filePath":"/fixture/SKILL.md","baseDir":"/fixture","sourceInfo":{"path":"/fixture","source":"fixture","scope":"project","origin":"top-level"},"disableModelInvocation":false});
    let mut steps = vec![json!({"op":"before"})];
    for tools in [
        vec![],
        vec!["read"],
        vec!["bash"],
        vec!["edit"],
        vec!["write"],
        vec!["other"],
        vec!["write", "bash", "read", "edit", "read"],
    ] {
        for append in [None, Some(""), Some("user requirements\nλ")] {
            for skills in [vec![], vec![skill("testing"), skill("λ")]] {
                steps.push(json!({"op":"before","prompt":"custom base\n","options":{"selectedTools":tools,"appendSystemPrompt":append,"skills":skills}}));
            }
        }
    }
    let result = harness::compare(
        "prompt-customizer.ts",
        harness::factory(example::prompt_customizer_extension),
        json!({"steps":steps}),
    )
    .await;
    assert_eq!(result["cases"].as_array().unwrap().len(), 43);
    assert!(
        result["cases"][1]["result"]["systemPrompt"]
            .as_str()
            .unwrap()
            .starts_with("custom base\n\n## Extension-Added Context")
    );
}
