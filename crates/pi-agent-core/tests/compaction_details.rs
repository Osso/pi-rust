use pi_js_compat::js_json::JsString;
use std::fmt::Debug;

use pi_agent_core::harness::compaction::CompactionDetails;

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn preserves_source_shaped_fields_and_public_mutation() {
    let mut details = CompactionDetails {
        read_files: vec![JsString::from("src/a.ts"), JsString::from("")],
        modified_files: vec![JsString::from("src/b.ts"), JsString::from("src/b.ts")],
    };

    assert_eq!(details.read_files, ["src/a.ts", ""]);
    assert_eq!(details.modified_files, ["src/b.ts", "src/b.ts"]);
    assert_debug(&details);

    details.read_files.push(JsString::from("é/技能/😀\0"));
    details.modified_files = vec![JsString::from("changed\n")];

    assert_eq!(details.read_files, ["src/a.ts", "", "é/技能/😀\0"]);
    assert_eq!(details.modified_files, ["changed\n"]);
}

#[test]
fn preserves_empty_vectors_and_ordered_duplicate_values() {
    let details = CompactionDetails {
        read_files: Vec::new(),
        modified_files: vec![JsString::from("first"), JsString::from("first"), "".to_owned()],
    };

    assert!(details.read_files.is_empty());
    assert_eq!(details.modified_files, ["first", "first", ""]);
}
