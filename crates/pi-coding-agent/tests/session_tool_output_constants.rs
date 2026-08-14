use pi_coding_agent::core::session_tool_output::{
    MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES, TOOL_RESULT_TRUNCATION_MARKER,
};
use pi_coding_agent::{
    MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES as ROOT_MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES,
    TOOL_RESULT_TRUNCATION_MARKER as ROOT_TOOL_RESULT_TRUNCATION_MARKER,
};

#[test]
fn session_tool_output_constants_match_at_module_and_crate_root() {
    let module_max_bytes: usize = MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES;
    assert_eq!(module_max_bytes, 1_048_576);
    assert_eq!(module_max_bytes, ROOT_MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES);

    assert_eq!(TOOL_RESULT_TRUNCATION_MARKER, "\n[tool output truncated at 1 MiB]");
    assert_eq!(TOOL_RESULT_TRUNCATION_MARKER, ROOT_TOOL_RESULT_TRUNCATION_MARKER);
    assert_eq!(TOOL_RESULT_TRUNCATION_MARKER.len(), 33);
    assert!(TOOL_RESULT_TRUNCATION_MARKER.is_ascii());
    assert_eq!(TOOL_RESULT_TRUNCATION_MARKER.as_bytes().first(), Some(&b'\n'));
    assert_eq!(TOOL_RESULT_TRUNCATION_MARKER.as_bytes().last(), Some(&b']'));
    assert!(!TOOL_RESULT_TRUNCATION_MARKER.ends_with('\n'));
}
