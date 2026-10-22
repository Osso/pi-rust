#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::AgentTool;
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::tools::path_utils::resolve_read_tool_path;
use pi_agent_core::harness::tools::read::ReadTool;
use pi_agent_core::harness::tools::tool_context::ExecutionToolContext;
use pi_agent_core::harness::types::FileSystem;
use pi_ai::types::TextOrImageContent;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

#[tokio::test]
async fn ts_created_raw_loader_paths_keep_names_locations_and_diagnostics_native() {
    use pi_agent_core::harness::prompt_templates::load_prompt_templates;
    use pi_agent_core::harness::skills::load_skills;
    use pi_agent_core::harness::system_prompt::format_skills_for_system_prompt;
    let root = tempfile::tempdir().unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "harness-raw-path-oracle.ts",
        "packages/ai/src/utils/sanitize-unicode.ts",
    );
    let reference = oracle.request_lossless(&pi_js_compat::js_json!({"root": root.path().to_str().unwrap()}));
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.path().to_str().unwrap().into(),
        ..Default::default()
    });
    assert_eq!(oracle.request(&serde_json::json!({"probe":true}))["ready"], true);
    let prompts = load_prompt_templates(&env, &[reference["prompt"].as_string().unwrap()]).await;
    assert!(prompts.prompt_templates[0].name.units().contains(&0xd800));
    let skills = load_skills(&env, &[reference["skillRoot"].as_string().unwrap()])
        .await
        .unwrap();
    assert!(skills.skills[0].name.units().contains(&0xdfff));
    assert!(skills.skills[0].file_path.units().contains(&0xdfff));
    assert!(
        skills
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.path.units().contains(&0xdfff))
    );
    assert!(
        format_skills_for_system_prompt(&skills.skills)
            .units()
            .contains(&0xdfff)
    );
}

#[tokio::test]
async fn ts_raw_path_stays_native_through_read_variants_env_and_tool() {
    let root = tempfile::tempdir().unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "harness-raw-path-oracle.ts",
        "packages/ai/src/utils/sanitize-unicode.ts",
    );
    let reference = oracle.request_lossless(&pi_js_compat::js_json!({ "root": root.path().to_str().unwrap() }));
    let input = reference["input"].as_string().unwrap().clone();
    let expected = reference["resolved"].as_string().unwrap().clone();
    let env = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.path().to_str().unwrap().into(),
        ..Default::default()
    }));
    let resolved = resolve_read_tool_path(env.as_ref(), &input, None).await.unwrap();
    assert_eq!(resolved, expected);
    assert!(resolved.units().contains(&0xd800));
    assert_eq!(env.file_info(&resolved).await.unwrap().path, expected);
    let tool = ReadTool::new(Arc::new(ExecutionToolContext::new(env.clone())));
    let result = tool
        .execute(
            "read".into(),
            pi_js_compat::js_json!({ "path": input }),
            None,
            Arc::new(|_| {}),
            None,
        )
        .await
        .unwrap();
    let TextOrImageContent::Text(text) = &result.content[0] else {
        panic!("read text result")
    };
    assert_eq!(text.text, "native path file\n");
    let absent = expected.clone() + ".missing";
    let error = env.read_binary_file(&absent, None).await.unwrap_err();
    assert_eq!(error.path.as_ref(), Some(&absent));
    assert!(error.message.contains(&JsString::from_utf16(vec![0xd800])));
}
