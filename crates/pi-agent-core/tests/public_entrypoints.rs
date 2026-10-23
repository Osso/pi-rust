use pi_agent_core::harness::env::NodeExecutionEnvOptions;
use pi_agent_core::node::NodeExecutionEnv;
use pi_agent_core::{
    CreateDirOptions, CreateTempFileOptions, FileContent, FileSystem, OptionalPatch, ReadTextLinesOptions,
    RemoveOptions, ShellExecResult,
};
use pi_js_compat::js_json::JsString;

#[tokio::test]
async fn frozen_root_types_and_node_entrypoint_support_the_same_filesystem_contract() {
    let root = tempfile::tempdir().unwrap();
    let environment = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.path().to_str().unwrap().into(),
        ..Default::default()
    });
    environment
        .create_dir(&"nested".into(), CreateDirOptions::default())
        .await
        .unwrap();
    environment
        .write_file(&"nested/file".into(), FileContent::Text(&"one\ntwo".into()), None)
        .await
        .unwrap();
    assert_eq!(
        environment
            .read_text_lines(
                &"nested/file".into(),
                ReadTextLinesOptions {
                    max_lines: Some(1.0),
                    abort: None
                }
            )
            .await
            .unwrap(),
        ["one"]
    );
    environment
        .remove(
            &"nested".into(),
            RemoveOptions {
                recursive: Some(true),
                force: None,
            },
        )
        .await
        .unwrap();
    assert!(!root.path().join("nested").exists());
    let temporary = environment
        .create_temp_file(CreateTempFileOptions {
            prefix: Some("pi-root-api-".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(std::path::Path::new(&temporary.to_utf8().unwrap()).is_file());
    std::fs::remove_dir_all(std::path::Path::new(&temporary.to_utf8().unwrap()).parent().unwrap()).unwrap();
    let patch = OptionalPatch::Set(JsString::from("native"));
    assert!(matches!(patch, OptionalPatch::Set(value) if value == "native"));
    let result = ShellExecResult {
        stdout: "out".into(),
        stderr: String::new(),
        exit_code: 0,
    };
    assert_eq!(result.stdout, "out");
}
