use pi_coding_agent::parse_skill_block;
use serde_json::{Value, json};

use super::oracle::TsOracle;

const FIRST_REQUEST_ID: u64 = 13_138;
const EXPECTED_FIXTURE_COUNT: usize = 10;

struct Fixture {
    request_id: u64,
    name: &'static str,
    cases: Vec<Case>,
}

struct Case {
    name: &'static str,
    text: &'static str,
}

pub(crate) fn compare_all(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "skill-block mismatch: {}", fixture.name);
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
        "operation": "parse_skill_block",
        "cases": fixture.cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({"name": case.name, "text": case.text})
}

fn rust_response(fixture: &Fixture) -> Value {
    let results = fixture.cases.iter().map(normalize_case).collect::<Vec<_>>();
    json!({
        "id": fixture.request_id,
        "ok": true,
        "output": {"results": results},
    })
}

fn normalize_case(case: &Case) -> Value {
    let parsed = parse_skill_block(case.text).map(|block| {
        json!({
            "name": block.name,
            "location": block.location,
            "content": block.content,
            "userMessage": block.user_message,
        })
    });
    json!({"name": case.name, "parsed": parsed})
}

fn fixtures() -> Vec<Fixture> {
    vec![
        basic_blocks_fixture(),
        attribute_values_fixture(),
        first_quote_boundary_fixture(),
        first_closing_delimiter_fixture(),
        suffix_grammar_fixture(),
        javascript_trim_fixture(),
        exact_lf_fixture(),
        malformed_prefix_fixture(),
        trailing_text_fixture(),
        delimiter_content_fixture(),
    ]
}

fn basic_blocks_fixture() -> Fixture {
    fixture(
        13_138,
        "minimal, empty, multiline, and message blocks",
        vec![
            case("minimal empty content", "<skill name=\"n\" location=\"l\">\n\n</skill>"),
            case(
                "multiline content",
                "<skill name=\"demo\" location=\"/tmp/SKILL.md\">\nfirst\nsecond\n</skill>",
            ),
            case(
                "message suffix",
                "<skill name=\"demo\" location=\"loc\">\ncontent\n</skill>\n\nuser message",
            ),
        ],
    )
}

fn attribute_values_fixture() -> Fixture {
    fixture(
        13_139,
        "attribute newlines, NUL, Unicode, and non-double-quote characters",
        vec![
            case(
                "representable attribute and content values",
                "<skill name=\"na\nme\0😀\" location=\"lo\ncation\0é\">\nα\0\n😀\n</skill>",
            ),
            case(
                "single quotes remain attribute content",
                "<skill name=\"a'b\" location=\"c'd\">\ntext\n</skill>",
            ),
        ],
    )
}

fn first_quote_boundary_fixture() -> Fixture {
    fixture(
        13_140,
        "first quote boundaries",
        vec![
            case(
                "name quote must be followed by location attribute",
                "<skill name=\"name\"ignored\" location=\"l\">\nx\n</skill>",
            ),
            case(
                "location quote must be followed by opener delimiter",
                "<skill name=\"n\" location=\"l\"ignored>\nx\n</skill>",
            ),
            case(
                "quoted attributes end at the first double quote",
                "<skill name=\"n\" location=\"l\">\nx\n</skill>",
            ),
        ],
    )
}

fn first_closing_delimiter_fixture() -> Fixture {
    fixture(
        13_141,
        "earliest closing delimiter whose remainder completes the match",
        vec![case(
            "backtracks when the first remainder is invalid",
            "<skill name=\"n\" location=\"l\">\nfirst\n</skill>\nsecond\n</skill>",
        )],
    )
}

fn suffix_grammar_fixture() -> Fixture {
    fixture(
        13_142,
        "optional suffix grammar",
        vec![
            case(
                "single LF suffix is rejected",
                "<skill name=\"n\" location=\"l\">\nx\n</skill>\ny",
            ),
            case(
                "empty double LF suffix is rejected",
                "<skill name=\"n\" location=\"l\">\nx\n</skill>\n\n",
            ),
            case(
                "nonempty suffix trimmed to empty",
                "<skill name=\"n\" location=\"l\">\nx\n</skill>\n\n\n",
            ),
            case(
                "nonempty suffix is preserved after trim",
                "<skill name=\"n\" location=\"l\">\nx\n</skill>\n\n\ty\n",
            ),
        ],
    )
}

fn javascript_trim_fixture() -> Fixture {
    fixture(
        13_143,
        "JavaScript trim characters and non-trim control character",
        vec![
            case(
                "FEFF line separators and surrounding spaces",
                "<skill name=\"n\" location=\"l\">\nx\n</skill>\n\n\u{feff}\u{2028} message \u{2029}\u{feff}",
            ),
            case(
                "JavaScript whitespace-only suffix",
                "<skill name=\"n\" location=\"l\">\nx\n</skill>\n\n\u{feff}\u{2028}\u{2029}\t \r",
            ),
            case(
                "U+001C is not JavaScript trim whitespace",
                "<skill name=\"n\" location=\"l\">\nx\n</skill>\n\n\u{001c}",
            ),
        ],
    )
}

fn exact_lf_fixture() -> Fixture {
    fixture(
        13_144,
        "exact LF structural anchoring",
        vec![
            case(
                "CRLF after opener is rejected",
                "<skill name=\"n\" location=\"l\">\r\nx\n</skill>",
            ),
            case(
                "missing LF before closing delimiter is rejected",
                "<skill name=\"n\" location=\"l\">x</skill>",
            ),
            case(
                "CR inside content is preserved",
                "<skill name=\"n\" location=\"l\">\nline\rline\n</skill>",
            ),
        ],
    )
}

fn malformed_prefix_fixture() -> Fixture {
    fixture(
        13_145,
        "malformed prefixes attributes and closing delimiters",
        vec![
            case(
                "prefix before block",
                "prefix<skill name=\"n\" location=\"l\">\nx\n</skill>",
            ),
            case("empty name attribute", "<skill name=\"\" location=\"l\">\nx\n</skill>"),
            case(
                "empty location attribute",
                "<skill name=\"n\" location=\"\">\nx\n</skill>",
            ),
            case("missing name quote", "<skill name=\"n location=\"l\">\nx\n</skill>"),
            case("wrong closing tag", "<skill name=\"n\" location=\"l\">\nx\n</Skill>"),
        ],
    )
}

fn trailing_text_fixture() -> Fixture {
    fixture(
        13_146,
        "full-input anchoring after the closing tag",
        vec![
            case(
                "trailing text without suffix delimiter",
                "<skill name=\"n\" location=\"l\">\nx\n</skill>suffix",
            ),
            case(
                "trailing newline without suffix text",
                "<skill name=\"n\" location=\"l\">\nx\n</skill>\n",
            ),
            case(
                "exact suffix delimiter with trailing newline content",
                "<skill name=\"n\" location=\"l\">\nx\n</skill>\n\nmessage\n",
            ),
        ],
    )
}

fn delimiter_content_fixture() -> Fixture {
    fixture(
        13_147,
        "delimiter-shaped content and embedded punctuation",
        vec![
            case(
                "non-closing delimiter text remains content",
                "<skill name=\"n\" location=\"l\">\n<skill>\n</skillx>\n</skill>",
            ),
            case(
                "XML-sensitive attribute and content text",
                "<skill name=\"<&>\" location=\"/a?b=1&c=2\">\n<&>\n</skill>",
            ),
        ],
    )
}

fn case(name: &'static str, text: &'static str) -> Case {
    Case { name, text }
}

fn fixture(request_id: u64, name: &'static str, cases: Vec<Case>) -> Fixture {
    Fixture {
        request_id,
        name,
        cases,
    }
}
