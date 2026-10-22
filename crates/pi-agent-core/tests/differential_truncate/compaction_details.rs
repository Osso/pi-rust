use pi_agent_core::harness::compaction::CompactionDetails;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_156;
const EXPECTED_FIXTURE_COUNT: usize = 8;

struct Fixture {
    request_id: u64,
    name: &'static str,
    read_files: Vec<&'static str>,
    modified_files: Vec<&'static str>,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);

    for (offset, fixture) in fixtures.iter().enumerate() {
        assert_eq!(fixture.request_id, FIRST_REQUEST_ID + offset as u64);
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "compaction details mismatch: {}", fixture.name);
    }
}

fn request(fixture: &Fixture) -> Value {
    json!({
        "id": fixture.request_id,
        "operation": "compaction_details",
        "cases": [{
            "name": fixture.name,
            "readFiles": fixture.read_files,
            "modifiedFiles": fixture.modified_files,
        }],
    })
}

fn rust_response(fixture: &Fixture) -> Value {
    let details = CompactionDetails {
        read_files: fixture.read_files.iter().map(|value| JsString::from(*value)).collect(),
        modified_files: fixture
            .modified_files
            .iter()
            .map(|value| JsString::from(*value))
            .collect(),
    };
    ok_response(
        fixture.request_id,
        json!({
            "results": [{
                "name": fixture.name,
                "readFiles": details.read_files,
                "modifiedFiles": details.modified_files,
            }],
        }),
    )
}

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            request_id: 13_156,
            name: "empty vectors",
            read_files: vec![],
            modified_files: vec![],
        },
        Fixture {
            request_id: 13_157,
            name: "ordinary ordered values",
            read_files: vec!["src/a.ts", "src/b.ts"],
            modified_files: vec!["src/c.ts"],
        },
        Fixture {
            request_id: 13_158,
            name: "duplicates and empty entries",
            read_files: vec!["same", "", "same"],
            modified_files: vec!["changed", "changed", ""],
        },
        Fixture {
            request_id: 13_159,
            name: "unicode and nul",
            read_files: vec!["é/技能/😀\0"],
            modified_files: vec!["<a>&\"'"],
        },
        Fixture {
            request_id: 13_160,
            name: "embedded newlines",
            read_files: vec!["before\ninside\n"],
            modified_files: vec!["after\n"],
        },
        Fixture {
            request_id: 13_161,
            name: "long punctuation",
            read_files: vec!["a:b;c,d.e/f\\g"],
            modified_files: vec!["(x) [y] {z}"],
        },
        Fixture {
            request_id: 13_162,
            name: "mixed empty and unicode",
            read_files: vec!["", "α", "β", ""],
            modified_files: vec!["😀", "α", "😀"],
        },
        Fixture {
            request_id: 13_163,
            name: "trailing newlines",
            read_files: vec!["read\n\n"],
            modified_files: vec!["modified\n\n"],
        },
    ]
}
