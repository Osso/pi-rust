#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::tools::tool_definition_wrapper::{ToolOutputSpill, cap_tool_result_output};
use pi_js_compat::js_json::{self as json, JsString, JsValue, encode_node_utf8};

#[test]
fn result_cap_matches_frozen_for_json_details_and_raw_text() {
    let mut oracle = oracle::TsOracle::spawn(
        "tool-definition-wrapper-oracle.ts",
        "packages/coding-agent/src/core/tools/tool-definition-wrapper.ts",
    );
    let text = JsString::from("line\n".repeat(2500)) + JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
    for details in [
        pi_js_compat::js_json!(["first", {"raw": JsString::from_utf16(vec![0xd800])}]),
        pi_js_compat::js_json!(42),
        pi_js_compat::js_json!({"truncation": null}),
        pi_js_compat::js_json!({"keep": true}),
    ] {
        let result = AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(text.clone()))],
            details: Some(details),
            is_error: None,
            terminate: None,
        };
        let expected = oracle.request(&pi_js_compat::js_json!({"id": 1, "result": result}));
        assert_eq!(expected["ok"], pi_js_compat::js_json!(true), "{expected:?}");
        let mut spill = ToolOutputSpill::default();
        let mut actual = cap_tool_result_output(result, &mut spill).expect("cap");
        let spilled = spill.full_output_path.as_ref().map(|path| {
            let bytes = std::fs::read(encode_node_utf8(path)).expect("spill bytes");
            std::fs::remove_file(encode_node_utf8(path)).expect("remove spill");
            if let TextOrImageContent::Text(content) = &mut actual.content[0] {
                content.text = content.text.replace(path, "<spill>");
            }
            actual
                .details
                .as_mut()
                .expect("details")
                .as_object_mut()
                .expect("object")
                .insert("fullOutputPath".into(), pi_js_compat::js_json!("<spill>"));
            bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        });
        assert_eq!(json::to_value(&actual).unwrap(), expected["output"]["result"]);
        assert_eq!(
            spilled.map(|value| JsValue::String(value.into())),
            expected["output"].as_object().unwrap().get("spilled").cloned()
        );
    }
}
