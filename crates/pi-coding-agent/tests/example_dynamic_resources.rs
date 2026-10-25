#[path = "../examples/extensions/dynamic-resources/index.rs"]
mod example;
use crate::shared_extension_examples_e as harness;
use pi_coding_agent::core::extensions::{ExtensionApi, HandlerError};
use serde_json::json;

fn register(api: &ExtensionApi) -> Result<(), HandlerError> {
    example::dynamic_resources_extension(
        api,
        std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/extensions/dynamic-resources"
        )),
    )
}

#[tokio::test]
async fn startup_and_reload_return_complete_paths_with_real_assets() {
    let factory = harness::factory(register);
    let result = harness::compare(
        "dynamic-resources/index.ts",
        factory,
        json!({"steps":[{"op":"resources","reason":"startup"},{"op":"resources","reason":"reload"}]}),
    )
    .await;
    for i in [0, 1] {
        assert_eq!(
            result["cases"][i]["result"]["skillPaths"][0]["path"],
            "<module>/SKILL.md"
        );
    }
    let lock: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/source-lock.json"
    )))
    .unwrap();
    let pinned = std::path::PathBuf::from(lock["worktree"].as_str().unwrap())
        .join("packages/coding-agent/examples/extensions/dynamic-resources");
    let native = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/extensions/dynamic-resources"
    ));
    for asset in ["SKILL.md", "dynamic.md", "dynamic.json"] {
        assert_eq!(
            std::fs::read(native.join(asset)).unwrap(),
            std::fs::read(pinned.join(asset)).unwrap()
        );
    }
}
