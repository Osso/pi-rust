use pi_agent_core::harness::tools::edit_diff::{
    LineEnding, detect_line_ending, normalize_to_lf, restore_line_endings, strip_bom,
};
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_081;
const EXPECTED_FIXTURE_COUNT: usize = 8;

struct Fixture {
    request_id: u64,
    name: &'static str,
    cases: Vec<Case>,
}

struct Case {
    name: &'static str,
    detection_input: &'static str,
    normalization_input: &'static str,
    restoration_input: &'static str,
    restoration_ending: LineEnding,
    bom_input: &'static str,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "edit line-ending mismatch: {}", fixture.name);
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
        "operation": "edit_text_helpers",
        "cases": fixture.cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "detectionInput": case.detection_input,
        "normalizationInput": case.normalization_input,
        "restorationInput": case.restoration_input,
        "restorationEnding": case.restoration_ending.as_str(),
        "bomInput": case.bom_input,
    })
}

fn rust_response(fixture: &Fixture) -> Value {
    let results = fixture
        .cases
        .iter()
        .map(|case| {
            let stripped = strip_bom(case.bom_input);
            json!({
                "name": case.name,
                "detectedEnding": detect_line_ending(case.detection_input).as_str(),
                "normalized": normalize_to_lf(case.normalization_input),
                "restored": restore_line_endings(case.restoration_input, case.restoration_ending),
                "stripped": {
                    "bom": stripped.bom,
                    "text": stripped.text,
                },
            })
        })
        .collect::<Vec<_>>();
    ok_response(fixture.request_id, json!({"results": results}))
}

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            request_id: 13_081,
            name: "empty and no-newline inputs",
            cases: vec![
                Case {
                    name: "empty-all",
                    detection_input: "",
                    normalization_input: "",
                    restoration_input: "",
                    restoration_ending: LineEnding::Lf,
                    bom_input: "plain",
                },
                Case {
                    name: "plain-no-newline",
                    detection_input: "plain text",
                    normalization_input: "plain text",
                    restoration_input: "a\nb",
                    restoration_ending: LineEnding::Lf,
                    bom_input: "text",
                },
            ],
        },
        Fixture {
            request_id: 13_082,
            name: "crlf-first ordinary conversion and one leading bom",
            cases: vec![Case {
                name: "crlf-before-lf",
                detection_input: "first\r\nsecond\nthird",
                normalization_input: "a\r\nb\r\n",
                restoration_input: "a\nb",
                restoration_ending: LineEnding::CrLf,
                bom_input: "\u{feff}leading",
            }],
        },
        Fixture {
            request_id: 13_083,
            name: "lf-first mixed conversion doubled restoration and repeated boms",
            cases: vec![Case {
                name: "lf-before-crlf",
                detection_input: "first\nsecond\r\n",
                normalization_input: "a\r\nb\rc\n",
                restoration_input: "a\r\nb\n",
                restoration_ending: LineEnding::CrLf,
                bom_input: "\u{feff}\u{feff}text\u{feff}",
            }],
        },
        Fixture {
            request_id: 13_084,
            name: "lone cr repeated sequences and interior bom",
            cases: vec![Case {
                name: "lone-cr-and-repeated-crlf",
                detection_input: "a\rb\rc",
                normalization_input: "\r\r\n",
                restoration_input: "\n\n\n",
                restoration_ending: LineEnding::CrLf,
                bom_input: "a\u{feff}b",
            }],
        },
        Fixture {
            request_id: 13_085,
            name: "repeated crlf lf identity and bom-only input",
            cases: vec![Case {
                name: "crlf-only-and-bom-only",
                detection_input: "\r\n\r\n",
                normalization_input: "\r\n\r\n",
                restoration_input: "a\r\nb",
                restoration_ending: LineEnding::Lf,
                bom_input: "\u{feff}",
            }],
        },
        Fixture {
            request_id: 13_086,
            name: "unicode controls and leading interior boms",
            cases: vec![Case {
                name: "unicode-control-bom-corpus",
                detection_input: "前😀e\u{301}\0\u{feff}\r\n後",
                normalization_input: "前😀e\u{301}\0\u{feff}\r\n中\r後",
                restoration_input: "前😀e\u{301}\0\u{feff}\n後",
                restoration_ending: LineEnding::CrLf,
                bom_input: "\u{feff}前😀e\u{301}\0\u{feff}",
            }],
        },
        Fixture {
            request_id: 13_087,
            name: "empty lines repeated line endings and bom placements",
            cases: vec![Case {
                name: "empty-line-sequence",
                detection_input: "\n\r\n\r\r\n\n",
                normalization_input: "\n\r\n\r\r\n\n",
                restoration_input: "\n\n\r\n",
                restoration_ending: LineEnding::CrLf,
                bom_input: "\u{feff}\n\u{feff}\r\n\u{feff}",
            }],
        },
        Fixture {
            request_id: 13_088,
            name: "deterministic mixed corpus",
            cases: vec![
                Case {
                    name: "corpus-plain-lf",
                    detection_input: "alpha\nbeta",
                    normalization_input: "a\r\nb",
                    restoration_input: "a\nb",
                    restoration_ending: LineEnding::Lf,
                    bom_input: "alpha",
                },
                Case {
                    name: "corpus-crlf-bom",
                    detection_input: "alpha\r\nbeta\n",
                    normalization_input: "\r\r\n",
                    restoration_input: "\n",
                    restoration_ending: LineEnding::CrLf,
                    bom_input: "\u{feff}x",
                },
                Case {
                    name: "corpus-unicode-nul",
                    detection_input: "x\ry",
                    normalization_input: "😀\r\n\0",
                    restoration_input: "前\n後",
                    restoration_ending: LineEnding::CrLf,
                    bom_input: "x\u{feff}y",
                },
            ],
        },
    ]
}
