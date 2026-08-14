use pi_coding_agent::core::tools::truncate::{
    DEFAULT_MAX_BYTES as MODULE_DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES as MODULE_DEFAULT_MAX_LINES, GREP_MAX_LINE_LENGTH,
};
use pi_coding_agent::core::tools::{
    DEFAULT_MAX_BYTES as TOOLS_DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES as TOOLS_DEFAULT_MAX_LINES,
};
use pi_coding_agent::{DEFAULT_MAX_BYTES as ROOT_DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES as ROOT_DEFAULT_MAX_LINES};

#[test]
fn truncation_constants_match_values_and_export_boundaries() {
    let module_max_lines: usize = MODULE_DEFAULT_MAX_LINES;
    let module_max_bytes: usize = MODULE_DEFAULT_MAX_BYTES;
    let grep_max_line_length: usize = GREP_MAX_LINE_LENGTH;
    let tools_max_lines: usize = TOOLS_DEFAULT_MAX_LINES;
    let tools_max_bytes: usize = TOOLS_DEFAULT_MAX_BYTES;
    let root_max_lines: usize = ROOT_DEFAULT_MAX_LINES;
    let root_max_bytes: usize = ROOT_DEFAULT_MAX_BYTES;

    assert_eq!(module_max_lines, 2_000);
    assert_eq!(module_max_bytes, 10_240);
    assert_eq!(grep_max_line_length, 500);
    assert_eq!(tools_max_lines, 2_000);
    assert_eq!(tools_max_bytes, 10_240);
    assert_eq!(root_max_lines, 2_000);
    assert_eq!(root_max_bytes, 10_240);

    assert_eq!(module_max_lines, tools_max_lines);
    assert_eq!(module_max_lines, root_max_lines);
    assert_eq!(module_max_bytes, tools_max_bytes);
    assert_eq!(module_max_bytes, root_max_bytes);
}
