use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::tools::edit::EditTool;
use pi_js_compat::js_json as json;
use std::sync::Arc;

#[tokio::test]
async fn typed_out_edit_fields_fail_instead_of_becoming_empty_replacements() {
    let cases = [
        json!([{"oldText":"before"}]),
        json!([{"oldText":"before","newText":false}]),
        json!([{"oldText":null,"newText":"after"}]),
        json!([null]),
        json!([{"oldText":"before","newText":"after"},{"oldText":"keep","newText":null}]),
    ];
    for (index, edits) in cases.into_iter().enumerate() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("file.txt");
        std::fs::write(&path, b"before\nkeep\n").unwrap();
        let result = EditTool::new(root.path().to_str().unwrap())
            .execute(
                "invalid-edit".into(),
                json!({"path":"file.txt","edits":edits}),
                None,
                Arc::new(|_| Ok(())),
                None,
            )
            .await;
        assert!(
            result.is_err(),
            "excluded invalid fields must fail, case {index}: {result:?}"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"before\nkeep\n",
            "invalid fields must not silently erase content, case {index}"
        );
    }
}
