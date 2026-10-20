#[path = "../examples/extensions/dynamic-resources/index.rs"]
mod example;
#[path = "support/extension_examples_e.rs"]
mod harness;
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
        assert_eq!(result["cases"][i]["result"]["skillPaths"][0]["path"], "<tmp>/SKILL.md");
    }
    let pinned = std::path::Path::new(
        "/home/osso/.worktrees/pi-rust-source-532f7d12/packages/coding-agent/examples/extensions/dynamic-resources",
    );
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
