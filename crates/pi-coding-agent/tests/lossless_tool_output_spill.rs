use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::tools::tool_definition_wrapper::{ToolOutputSpill, cap_tool_result_output};
use pi_js_compat::js_json::{JsString, encode_node_utf8};

#[test]
fn spill_path_remains_raw_in_footer_and_details_but_encodes_for_the_file() {
    let temp = tempfile::tempdir().unwrap();
    let path = JsString::from(temp.path().to_str().unwrap()) + "/spill-" + JsString::from_utf16(vec![0xd800]);
    let text = JsString::from("line\n".repeat(4_000));
    let mut spill = ToolOutputSpill {
        full_output_path: Some(path.clone()),
        error: None,
    };
    let capped = cap_tool_result_output(
        AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(text.clone()))],
            details: None,
            is_error: None,
            terminate: None,
        },
        &mut spill,
    )
    .unwrap();
    let TextOrImageContent::Text(output) = &capped.content[0] else {
        panic!("text result")
    };
    assert!(output.text.contains(&path));
    assert_eq!(capped.details.unwrap()["fullOutputPath"].as_string(), Some(&path));
    assert_eq!(spill.full_output_path, Some(path.clone()));
    assert_eq!(
        std::fs::read(encode_node_utf8(&path)).unwrap(),
        encode_node_utf8(&text).as_bytes()
    );
}
