use pi_agent_core::harness::compaction::utils::format_file_operations;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_089;
const EXPECTED_FIXTURE_COUNT: usize = 7;

struct Fixture {
    request_id: u64,
    name: &'static str,
    read_files: Vec<String>,
    modified_files: Vec<String>,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "file-operation mismatch: {}", fixture.name);
    }
}

fn assert_request_ids(fixtures: &[Fixture]) {
    for (offset, fixture) in fixtures.iter().enumerate() {
        let expected = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        assert_eq!(fixture.request_id, expected, "fixture IDs must remain contiguous");
    }
}

fn request(fixture: &Fixture) -> Value {
    json!({
        "id": fixture.request_id,
        "operation": "file_operations_format",
        "readFiles": fixture.read_files,
        "modifiedFiles": fixture.modified_files,
    })
}

fn rust_response(fixture: &Fixture) -> Value {
    ok_response(
        fixture.request_id,
        json!(format_file_operations(&fixture.read_files, &fixture.modified_files)),
    )
}

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            request_id: 13_089,
            name: "empty read and modified lists",
            read_files: Vec::new(),
            modified_files: Vec::new(),
        },
        Fixture {
            request_id: 13_090,
            name: "read-only files",
            read_files: vec!["src/a.ts".to_owned(), "src/b.ts".to_owned()],
            modified_files: Vec::new(),
        },
        Fixture {
            request_id: 13_091,
            name: "modified-only files",
            read_files: Vec::new(),
            modified_files: vec!["src/changed.ts".to_owned()],
        },
        Fixture {
            request_id: 13_092,
            name: "both sections and exact separators",
            read_files: vec!["src/read.ts".to_owned()],
            modified_files: vec!["src/changed.ts".to_owned()],
        },
        Fixture {
            request_id: 13_093,
            name: "multiple entries preserve order and duplicates",
            read_files: vec!["z.ts".to_owned(), "a.ts".to_owned(), "z.ts".to_owned()],
            modified_files: vec!["m.ts".to_owned(), "m.ts".to_owned(), "b.ts".to_owned()],
        },
        Fixture {
            request_id: 13_094,
            name: "empty unicode nul punctuation and raw xml text",
            read_files: vec!["".to_owned(), "é/技能/😀\0<&>\"'".to_owned()],
            modified_files: vec!["<raw>&\"'".to_owned(), "punctuation: []{}()".to_owned()],
        },
        Fixture {
            request_id: 13_095,
            name: "many entries with embedded and trailing newlines",
            read_files: vec![
                "before\ninside\n\nafter".to_owned(),
                "read-1".to_owned(),
                "read-2\n".to_owned(),
                "read-3".to_owned(),
            ],
            modified_files: vec![
                "changed\n\n".to_owned(),
                "write-1".to_owned(),
                "write-2\ntrailing\n".to_owned(),
                "write-3".to_owned(),
            ],
        },
    ]
}
