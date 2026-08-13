use std::fmt::Debug;

use pi_agent_core::harness::compaction::CompactionDetails;

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn preserves_source_shaped_fields_and_public_mutation() {
    let mut details = CompactionDetails {
        read_files: vec!["src/a.ts".to_owned(), "".to_owned()],
        modified_files: vec!["src/b.ts".to_owned(), "src/b.ts".to_owned()],
    };

    assert_eq!(details.read_files, ["src/a.ts", ""]);
    assert_eq!(details.modified_files, ["src/b.ts", "src/b.ts"]);
    assert_debug(&details);

    details.read_files.push("é/技能/😀\0".to_owned());
    details.modified_files = vec!["changed\n".to_owned()];

    assert_eq!(details.read_files, ["src/a.ts", "", "é/技能/😀\0"]);
    assert_eq!(details.modified_files, ["changed\n"]);
}

#[test]
fn preserves_empty_vectors_and_ordered_duplicate_values() {
    let details = CompactionDetails {
        read_files: Vec::new(),
        modified_files: vec!["first".to_owned(), "first".to_owned(), "".to_owned()],
    };

    assert!(details.read_files.is_empty());
    assert_eq!(details.modified_files, ["first", "first", ""]);
}
