use pi_agent_core::harness::compaction::utils::format_file_operations;

#[test]
fn empty_lists_return_empty_output() {
    let read_files: Vec<String> = Vec::new();
    let modified_files: Vec<String> = Vec::new();

    assert_eq!(format_file_operations(&read_files, &modified_files), "");
}

#[test]
fn read_only_files_emit_one_exact_section() {
    let read_files = vec!["src/a.ts".to_owned(), "src/b.ts".to_owned()];
    let modified_files: Vec<String> = Vec::new();

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<read-files>\nsrc/a.ts\nsrc/b.ts\n</read-files>"
    );
}

#[test]
fn modified_only_files_emit_one_exact_section() {
    let read_files: Vec<String> = Vec::new();
    let modified_files = vec!["src/changed.ts".to_owned()];

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<modified-files>\nsrc/changed.ts\n</modified-files>"
    );
}

#[test]
fn both_sections_have_exact_prefix_and_separator() {
    let read_files = vec!["src/read.ts".to_owned()];
    let modified_files = vec!["src/write.ts".to_owned()];

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<read-files>\nsrc/read.ts\n</read-files>\n\n<modified-files>\nsrc/write.ts\n</modified-files>"
    );
}

#[test]
fn preserves_order_duplicates_and_empty_entries() {
    let read_files = vec!["first".to_owned(), "".to_owned(), "first".to_owned(), "last".to_owned()];
    let modified_files = vec!["changed".to_owned(), "changed".to_owned(), "".to_owned()];

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<read-files>\nfirst\n\nfirst\nlast\n</read-files>\n\n<modified-files>\nchanged\nchanged\n\n</modified-files>"
    );
}

#[test]
fn preserves_unicode_nul_punctuation_and_raw_xml_sensitive_content() {
    let read_files = vec!["é/技能/😀\0<&>\"'".to_owned()];
    let modified_files = vec!["a&b< c >\"'".to_owned()];

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<read-files>\né/技能/😀\0<&>\"'\n</read-files>\n\n<modified-files>\na&b< c >\"'\n</modified-files>"
    );
}

#[test]
fn preserves_embedded_and_trailing_newlines_verbatim() {
    let read_files = vec!["before\ninside\n".to_owned(), "after".to_owned()];
    let modified_files = vec!["changed\n\n".to_owned()];

    assert_eq!(
        format_file_operations(&read_files, &modified_files),
        "\n\n<read-files>\nbefore\ninside\n\nafter\n</read-files>\n\n<modified-files>\nchanged\n\n\n</modified-files>"
    );
}
