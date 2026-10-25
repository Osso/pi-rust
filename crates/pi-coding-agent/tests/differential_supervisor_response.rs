use crate::shared_oracle as oracle;

use pi_coding_agent::core::runtime_message_markers::{
    RuntimeMessageMarker, is_duplicate_turn_assistant_message, is_duplicate_turn_guard_message,
};
use pi_coding_agent::supervisor::response_tool::{
    SUPERVISOR_RESPONSE_TOOL_NAME, create_supervisor_response_tool, supervisor_response_parameters,
    supervisor_response_text,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

fn marker_name(marker: Option<RuntimeMessageMarker>) -> Option<&'static str> {
    marker.map(RuntimeMessageMarker::as_str)
}

#[test]
fn runtime_message_marker_predicates_match_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn(
        "runtime-message-markers-oracle.ts",
        "packages/coding-agent/src/core/runtime-message-markers.ts",
    );

    for (id, marker) in [
        (28_400, None),
        (28_401, Some(RuntimeMessageMarker::DuplicateTurnAssistant)),
        (28_402, Some(RuntimeMessageMarker::DuplicateTurnGuard)),
    ] {
        let expected = oracle.request(&json!({
            "id": id,
            "operation": "predicates",
            "marker": marker_name(marker),
        }));
        let actual = json!({
            "id": id,
            "ok": true,
            "output": {
                "stored": Value::Null,
                "assistant": is_duplicate_turn_assistant_message(None, marker),
                "guard": is_duplicate_turn_guard_message(None, marker),
            },
        });
        assert_eq!(expected, actual, "marker: {marker:?}");
    }
}

#[test]
fn runtime_message_marker_literals_match_the_source() {
    assert_eq!(
        RuntimeMessageMarker::DuplicateTurnAssistant.as_str(),
        "duplicate_turn_assistant"
    );
    assert_eq!(
        RuntimeMessageMarker::DuplicateTurnGuard.as_str(),
        "duplicate_turn_guard"
    );
}

#[test]
fn stored_markers_are_honored_like_the_source_weak_map_lookup() {
    assert!(is_duplicate_turn_assistant_message(
        Some(RuntimeMessageMarker::DuplicateTurnAssistant),
        None
    ));
    assert!(!is_duplicate_turn_guard_message(
        Some(RuntimeMessageMarker::DuplicateTurnAssistant),
        None
    ));
    assert!(is_duplicate_turn_guard_message(
        Some(RuntimeMessageMarker::DuplicateTurnGuard),
        None
    ));
    assert!(!is_duplicate_turn_assistant_message(
        Some(RuntimeMessageMarker::DuplicateTurnGuard),
        None
    ));
}

#[test]
fn supervisor_response_metadata_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn(
        "supervisor-response-tool-oracle.ts",
        "packages/coding-agent/src/supervisor/response-tool.ts",
    );
    let expected = oracle.request(&json!({"id": 28_410, "operation": "metadata"}));

    let tool = create_supervisor_response_tool();
    let actual = json!({
        "id": 28_410,
        "ok": true,
        "output": {
            "constantName": SUPERVISOR_RESPONSE_TOOL_NAME,
            "name": tool.name,
            "label": tool.label,
            "description": tool.description,
            "promptSnippet": tool.prompt_snippet,
            "promptGuidelines": tool.prompt_guidelines,
            "parameters": supervisor_response_parameters(),
            "executionMode": tool.execution_mode.map(|mode| mode.as_str()),
        },
    });
    assert_eq!(expected, actual);
}

#[test]
fn supervisor_response_text_and_result_shape_match_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn(
        "supervisor-response-tool-oracle.ts",
        "packages/coding-agent/src/supervisor/response-tool.ts",
    );

    let cases: Vec<(u64, Value)> = vec![
        (28_420, json!({"kind": "approve"})),
        (28_421, json!({"kind": "reject", "reason": "nope"})),
        (28_422, json!({"kind": "advisory", "answer": "  keep going  "})),
        (28_423, json!({"kind": "advisory", "answer": "   "})),
        (28_424, json!({"kind": "advisory"})),
        (28_425, json!({"kind": "set", "objective": "ship it"})),
        (28_426, json!({"kind": "error", "instructions": "fix the build"})),
    ];

    for (id, params) in cases {
        let expected = oracle.request(&json!({"id": id, "operation": "execute", "params": params}));
        let output = &expected["output"];
        let text = output["content"][0]["text"].as_string().expect("oracle text content");
        assert_eq!(*text, supervisor_response_text(&params), "params: {params:?}");
        assert_eq!(output["details"], params, "details: {params:?}");
        assert_eq!(output["terminate"], json!(true));
        assert_eq!(expected["ok"], json!(true));
    }
}
