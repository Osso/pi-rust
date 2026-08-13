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
        empty_fixture(),
        read_only_fixture(),
        modified_only_fixture(),
        both_sections_fixture(),
        ordering_fixture(),
        raw_content_fixture(),
        newline_fixture(),
    ]
}

fn empty_fixture() -> Fixture {
    fixture(13_089, "empty read and modified lists", &[], &[])
}

fn read_only_fixture() -> Fixture {
    fixture(13_090, "read-only files", &["src/a.ts", "src/b.ts"], &[])
}

fn modified_only_fixture() -> Fixture {
    fixture(13_091, "modified-only files", &[], &["src/changed.ts"])
}

fn both_sections_fixture() -> Fixture {
    fixture(
        13_092,
        "both sections and exact separators",
        &["src/read.ts"],
        &["src/changed.ts"],
    )
}

fn ordering_fixture() -> Fixture {
    fixture(
        13_093,
        "multiple entries preserve order and duplicates",
        &["z.ts", "a.ts", "z.ts"],
        &["m.ts", "m.ts", "b.ts"],
    )
}

fn raw_content_fixture() -> Fixture {
    fixture(
        13_094,
        "empty unicode nul punctuation and raw xml text",
        &["", "é/技能/😀\0<&>\"'"],
        &["<raw>&\"'", "punctuation: []{}()"],
    )
}

fn newline_fixture() -> Fixture {
    fixture(
        13_095,
        "many entries with embedded and trailing newlines",
        &["before\ninside\n\nafter", "read-1", "read-2\n", "read-3"],
        &["changed\n\n", "write-1", "write-2\ntrailing\n", "write-3"],
    )
}

fn fixture(request_id: u64, name: &'static str, read_files: &[&str], modified_files: &[&str]) -> Fixture {
    Fixture {
        request_id,
        name,
        read_files: read_files.iter().map(|entry| (*entry).to_owned()).collect(),
        modified_files: modified_files.iter().map(|entry| (*entry).to_owned()).collect(),
    }
}
