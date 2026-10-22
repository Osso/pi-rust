use pi_agent_core::harness::compaction::utils::format_file_operations;
use pi_js_compat::js_json::JsString;

#[test]
fn empty_lists_return_empty_output() {
    let read_files: Vec<JsString> = Vec::new();
    let modified_files: Vec<JsString> = Vec::new();

    assert_eq!(format_file_operations(&read_files, &modified_files), "");
}

#[test]
fn read_only_files_emit_one_exact_section() {
    let read_files = vec![JsString::from("src/a.ts"), JsString::from("src/b.ts")];
    let modified_files: Vec<JsString> = Vec::new();

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<read-files>\nsrc/a.ts\nsrc/b.ts\n</read-files>"
    );
}

#[test]
fn modified_only_files_emit_one_exact_section() {
    let read_files: Vec<JsString> = Vec::new();
    let modified_files = vec![JsString::from("src/changed.ts")];

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<modified-files>\nsrc/changed.ts\n</modified-files>"
    );
}

#[test]
fn both_sections_have_exact_prefix_and_separator() {
    let read_files = vec![JsString::from("src/read.ts")];
    let modified_files = vec![JsString::from("src/write.ts")];

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<read-files>\nsrc/read.ts\n</read-files>\n\n<modified-files>\nsrc/write.ts\n</modified-files>"
    );
}

#[test]
fn preserves_order_duplicates_and_empty_entries() {
    let read_files = vec![
        JsString::from("first"),
        JsString::from(""),
        JsString::from("first"),
        JsString::from("last"),
    ];
    let modified_files = vec![JsString::from("changed"), JsString::from("changed"), JsString::from("")];

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<read-files>\nfirst\n\nfirst\nlast\n</read-files>\n\n<modified-files>\nchanged\nchanged\n\n</modified-files>"
    );
}

#[test]
fn preserves_unicode_nul_punctuation_and_raw_xml_sensitive_content() {
    let read_files = vec![JsString::from("é/技能/😀\0<&>\"'")];
    let modified_files = vec![JsString::from("a&b< c >\"'")];

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<read-files>\né/技能/😀\0<&>\"'\n</read-files>\n\n<modified-files>\na&b< c >\"'\n</modified-files>"
    );
}

#[test]
fn preserves_embedded_and_trailing_newlines_verbatim() {
    let read_files = vec![JsString::from("before\ninside\n"), JsString::from("after")];
    let modified_files = vec![JsString::from("changed\n\n")];

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<read-files>\nbefore\ninside\n\nafter\n</read-files>\n\n<modified-files>\nchanged\n\n\n</modified-files>"
    );
}
