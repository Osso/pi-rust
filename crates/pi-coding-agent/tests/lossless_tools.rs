use pi_agent_core::agent::{AgentTool, AgentToolResult};
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::tools::tool_definition_wrapper::{ToolOutputSpill, cap_tool_result_output};
use pi_coding_agent::core::tools::write::WriteTool;
use pi_js_compat::js_json::{JsString, encode_node_utf8, parse_lossless};
use std::sync::Arc;

#[tokio::test]
async fn file_write_encodes_only_at_node_boundary_and_keeps_original_success_path() {
    let directory = tempfile::tempdir().unwrap();
    let tool = WriteTool::new(directory.path().to_str().unwrap());
    let params = parse_lossless(r#"{"path":"file-\ud800","content":"\udfff"}"#).unwrap();
    let result = tool
        .execute("write".into(), params, None, Arc::new(|_| {}), None)
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
    assert_eq!(std::fs::read(&path).unwrap(), encode_node_utf8(&text).as_bytes());
    std::fs::remove_file(path).unwrap();
}
