use crate::shared_ai_oracle as oracle;

use pi_agent_core::agent::{AgentTool, AgentToolResult};
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::tools::tool_definition_wrapper::{ToolOutputSpill, cap_tool_result_output};
use pi_coding_agent::core::tools::write::WriteTool;
use pi_js_compat::js_json::{JsString, encode_node_utf8, parse_lossless};
use std::sync::Arc;

#[test]
fn coding_edits_and_duplicate_context_match_frozen_utf16_results() {
    use pi_coding_agent::core::tools::edit_diff::{
        Edit, apply_edits_to_normalized_content, generate_diff_string, generate_unified_patch,
    };
    use pi_js_compat::js_json::JsValue;
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-edit-oracle.ts",
        "packages/coding-agent/src/core/tools/edit-diff.ts",
    );
    let path = JsString::from("file-") + JsString::from_utf16(vec![0xdfff]);
    for content in [JsString::from("😀"), JsString::from("😀".repeat(100))] {
        let old_text = JsString::from_utf16(vec![0xde00]);
        let new_text = JsString::from("x");
        let reference = oracle.request(&serde_json::json!({"content":content.units(),"oldText":old_text.units(),"newText":new_text.units(),"path":path.units()}));
        let actual = match apply_edits_to_normalized_content(&content, &[Edit { old_text, new_text }], &path) {
            Ok(applied) => {
                let result = generate_diff_string(&applied.base_content, &applied.new_content, 4);
                let mut diff = pi_js_compat::js_json!({"diff":result.diff});
                if let Some(line) = result.first_changed_line {
                    diff["firstChangedLine"] = pi_js_compat::js_json!(line);
                }
                let patch = generate_unified_patch(&path, &applied.base_content, &applied.new_content, 4);
                pi_js_compat::js_json!({"baseContent":applied.base_content,"newContent":applied.new_content,"diff":diff,"patch":patch})
            }
            Err(error) => pi_js_compat::js_json!({"error":error.message}),
        };
        let expected: JsValue = parse_lossless(reference["wire"].as_str().unwrap()).unwrap();
        assert_eq!(actual, expected);
    }
}

#[tokio::test]
async fn coding_file_edit_retains_raw_result_before_node_write_encoding() {
    use pi_coding_agent::core::tools::edit::EditTool;
    let directory = tempfile::tempdir().unwrap();
    let path = JsString::from("file-") + JsString::from_utf16(vec![0xd800]);
    let disk_path = directory.path().join(encode_node_utf8(&path));
    std::fs::write(&disk_path, "😀").unwrap();
    let tool = EditTool::new(directory.path().to_str().expect("utf-8 test path"));
    let params = parse_lossless(r#"{"path":"file-\ud800","edits":[{"oldText":"\ude00","newText":"x"}]}"#).unwrap();
    let result = tool
        .execute("edit".into(), params, None, Arc::new(|_| Ok(())), None)
        .await
        .unwrap();
    assert_eq!(std::fs::read(disk_path).unwrap(), "�x".as_bytes());
    assert!(
        result.details.as_ref().unwrap()["diff"]
            .as_string()
            .unwrap()
            .contains(JsString::from_utf16(vec![0xd83d]))
    );
    assert!(
        result.details.as_ref().unwrap()["patch"]
            .as_string()
            .unwrap()
            .contains(path)
    );
}

#[tokio::test]
async fn file_write_encodes_only_at_node_boundary_and_keeps_original_success_path() {
    let directory = tempfile::tempdir().unwrap();
    let tool = WriteTool::new(directory.path().to_str().expect("utf-8 test path"));
    let params = parse_lossless(r#"{"path":"file-\ud800","content":"\udfff"}"#).unwrap();
    let result = tool
        .execute("write".into(), params, None, Arc::new(|_| Ok(())), None)
        .await
        .unwrap();
    let path = JsString::from("file-") + JsString::from_utf16(vec![0xd800]);
    let persisted = directory.path().join(encode_node_utf8(&path));
    assert_eq!(std::fs::read(persisted).unwrap(), "�".as_bytes());
    let TextOrImageContent::Text(text) = &result.content[0] else {
        panic!("text result");
    };
    assert_eq!(text.text, JsString::from("Successfully wrote 1 bytes to ") + path);
}

#[test]
fn tool_spill_keeps_raw_tail_and_detail_keys_but_encodes_file_bytes_like_node() {
    let raw = JsString::from_utf16(vec![0xdfff]);
    let text = JsString::from("line\n".repeat(2000)) + &raw;
    let details = parse_lossless(r#"{"\ud800":"\udfff"}"#).unwrap();
    let result = AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text.clone()))],
        details: Some(details.clone()),
        is_error: None,
        terminate: None,
    };
    let mut spill = ToolOutputSpill::default();
    let capped = cap_tool_result_output(result, &mut spill).unwrap();
    assert_eq!(capped.details.as_ref().unwrap().get("\u{fffd}"), None);
    let object = capped.details.as_ref().unwrap().as_object().unwrap();
    assert_eq!(
        object.get(&JsString::from_utf16(vec![0xd800])),
        details.as_object().unwrap().get(&JsString::from_utf16(vec![0xd800]))
    );
    let TextOrImageContent::Text(text_result) = &capped.content[0] else {
        panic!("text result");
    };
    assert!(text_result.text.contains(&raw));
    let path = spill.full_output_path.unwrap();
    assert_eq!(
        std::fs::read(encode_node_utf8(&path)).unwrap(),
        encode_node_utf8(&text).as_bytes()
    );
    std::fs::remove_file(encode_node_utf8(&path)).unwrap();
}
