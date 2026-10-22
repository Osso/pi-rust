use pi_agent_core::harness::{PromptTemplateDiagnostic, PromptTemplateDiagnosticCode, PromptTemplateDiagnosticType};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_121;
const EXPECTED_FIXTURE_COUNT: usize = 8;

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    code: PromptTemplateDiagnosticCode,
    message: &'static str,
    path: &'static str,
}

#[derive(Clone, Copy)]
struct Fixture {
    request_id: u64,
    name: &'static str,
    cases: &'static [Case],
}

const ALL_CODES: [Case; 4] = [
    Case {
        name: "file info",
        code: PromptTemplateDiagnosticCode::FileInfoFailed,
        message: "could not inspect prompt template",
        path: "/prompts/info.md",
    },
    Case {
        name: "list",
        code: PromptTemplateDiagnosticCode::ListFailed,
        message: "could not list prompt templates",
        path: "/prompts",
    },
    Case {
        name: "read",
        code: PromptTemplateDiagnosticCode::ReadFailed,
        message: "could not read prompt template",
        path: "/prompts/read.md",
    },
    Case {
        name: "parse",
        code: PromptTemplateDiagnosticCode::ParseFailed,
        message: "could not parse prompt template",
        path: "/prompts/parse.md",
    },
];

const EMPTY_STRINGS: [Case; 4] = [
    Case {
        name: "empty file info",
        code: PromptTemplateDiagnosticCode::FileInfoFailed,
        message: "",
        path: "",
    },
    Case {
        name: "empty list",
        code: PromptTemplateDiagnosticCode::ListFailed,
        message: "",
        path: "",
    },
    Case {
        name: "empty read",
        code: PromptTemplateDiagnosticCode::ReadFailed,
        message: "",
        path: "",
    },
    Case {
        name: "empty parse",
        code: PromptTemplateDiagnosticCode::ParseFailed,
        message: "",
        path: "",
    },
];

const UNICODE_STRINGS: [Case; 4] = [
    Case {
        name: "unicode bmp",
        code: PromptTemplateDiagnosticCode::FileInfoFailed,
        message: "é漢字",
        path: "/é/漢字.md",
    },
    Case {
        name: "unicode supplementary",
        code: PromptTemplateDiagnosticCode::ListFailed,
        message: "😀𐐷",
        path: "😀/𐐷.md",
    },
    Case {
        name: "unicode combining",
        code: PromptTemplateDiagnosticCode::ReadFailed,
        message: "e\u{301}o\u{308}",
        path: "/cafe\u{301}.md",
    },
    Case {
        name: "unicode mixed scripts",
        code: PromptTemplateDiagnosticCode::ParseFailed,
        message: "ไทย Ελληνικά العربية 日本語",
        path: "/mixed/ไทย-Ελληνικά.md",
    },
];

const NUL_STRINGS: [Case; 4] = [
    Case {
        name: "nul message",
        code: PromptTemplateDiagnosticCode::FileInfoFailed,
        message: "before\0after",
        path: "/normal.md",
    },
    Case {
        name: "nul path",
        code: PromptTemplateDiagnosticCode::ListFailed,
        message: "ordinary",
        path: "/before\0after",
    },
    Case {
        name: "multiple nuls",
        code: PromptTemplateDiagnosticCode::ReadFailed,
        message: "a\0b\0c",
        path: "\0/root\0",
    },
    Case {
        name: "nul both",
        code: PromptTemplateDiagnosticCode::ParseFailed,
        message: "\0message\0",
        path: "\0path\0",
    },
];

const NEWLINE_STRINGS: [Case; 4] = [
    Case {
        name: "carriage return",
        code: PromptTemplateDiagnosticCode::FileInfoFailed,
        message: "before\rafter",
        path: "/before\rafter.md",
    },
    Case {
        name: "line feed",
        code: PromptTemplateDiagnosticCode::ListFailed,
        message: "before\nafter",
        path: "/before\nafter",
    },
    Case {
        name: "crlf",
        code: PromptTemplateDiagnosticCode::ReadFailed,
        message: "before\r\nafter",
        path: "/before\r\nafter.md",
    },
    Case {
        name: "trailing newline",
        code: PromptTemplateDiagnosticCode::ParseFailed,
        message: "message\n",
        path: "/path.md\n",
    },
];

const RAW_CONTENT: [Case; 4] = [
    Case {
        name: "xml punctuation",
        code: PromptTemplateDiagnosticCode::FileInfoFailed,
        message: "<warning>& \"quoted\" 'single'",
        path: "/<raw>&.md",
    },
    Case {
        name: "markdown punctuation",
        code: PromptTemplateDiagnosticCode::ListFailed,
        message: "# heading **bold** [link](url)",
        path: "/docs/[draft].md",
    },
    Case {
        name: "backslashes",
        code: PromptTemplateDiagnosticCode::ReadFailed,
        message: r"C:\\prompt\\read.md",
        path: r"C:\\prompt\\read.md",
    },
    Case {
        name: "mixed raw content",
        code: PromptTemplateDiagnosticCode::ParseFailed,
        message: "{}[]()<>:;,$^|~`",
        path: "/raw/{template}_$1.md",
    },
];

const DUPLICATE_VALUES: [Case; 5] = [
    Case {
        name: "duplicate first",
        code: PromptTemplateDiagnosticCode::FileInfoFailed,
        message: "same message",
        path: "/same/path.md",
    },
    Case {
        name: "duplicate second",
        code: PromptTemplateDiagnosticCode::ListFailed,
        message: "same message",
        path: "/same/path.md",
    },
    Case {
        name: "duplicate third",
        code: PromptTemplateDiagnosticCode::ReadFailed,
        message: "same message",
        path: "/same/path.md",
    },
    Case {
        name: "duplicate fourth",
        code: PromptTemplateDiagnosticCode::ParseFailed,
        message: "same message",
        path: "/same/path.md",
    },
    Case {
        name: "varied final",
        code: PromptTemplateDiagnosticCode::ReadFailed,
        message: "final message",
        path: "/final/path.md",
    },
];

const MIXED_BOUNDARIES: [Case; 4] = [
    Case {
        name: "mixed file info",
        code: PromptTemplateDiagnosticCode::FileInfoFailed,
        message: "é\0line\n<raw>",
        path: "/😀/file\r\n.md",
    },
    Case {
        name: "mixed list",
        code: PromptTemplateDiagnosticCode::ListFailed,
        message: "\n\0\r\n",
        path: "",
    },
    Case {
        name: "mixed read",
        code: PromptTemplateDiagnosticCode::ReadFailed,
        message: r"backslash\\ and `code`",
        path: "/日本語/[read].md\n",
    },
    Case {
        name: "mixed parse",
        code: PromptTemplateDiagnosticCode::ParseFailed,
        message: "final 😀 e\u{301}",
        path: "/<parse>&\0.md",
    },
];

const FIXTURES: [Fixture; EXPECTED_FIXTURE_COUNT] = [
    Fixture {
        request_id: 13_121,
        name: "all diagnostic codes",
        cases: &ALL_CODES,
    },
    Fixture {
        request_id: 13_122,
        name: "empty strings",
        cases: &EMPTY_STRINGS,
    },
    Fixture {
        request_id: 13_123,
        name: "unicode strings",
        cases: &UNICODE_STRINGS,
    },
    Fixture {
        request_id: 13_124,
        name: "embedded nul strings",
        cases: &NUL_STRINGS,
    },
    Fixture {
        request_id: 13_125,
        name: "newline strings",
        cases: &NEWLINE_STRINGS,
    },
    Fixture {
        request_id: 13_126,
        name: "raw punctuation content",
        cases: &RAW_CONTENT,
    },
    Fixture {
        request_id: 13_127,
        name: "duplicate and varied values",
        cases: &DUPLICATE_VALUES,
    },
    Fixture {
        request_id: 13_128,
        name: "mixed boundary corpus",
        cases: &MIXED_BOUNDARIES,
    },
];

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    assert_eq!(FIXTURES.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids();

    for fixture in FIXTURES {
        let expected = oracle.request(&request(&fixture));
        let actual = rust_response(&fixture);
        assert_eq!(actual, expected, "prompt diagnostic mismatch: {}", fixture.name);
    }
}

fn assert_request_ids() {
    for (offset, fixture) in FIXTURES.iter().enumerate() {
        let expected = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        assert_eq!(fixture.request_id, expected, "fixture IDs must remain contiguous");
    }
}

fn request(fixture: &Fixture) -> Value {
    json!({
        "id": fixture.request_id,
        "operation": "prompt_template_diagnostic",
        "cases": fixture.cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "type": PromptTemplateDiagnosticType::Warning.as_str(),
        "code": case.code.as_str(),
        "message": case.message,
        "path": case.path,
    })
}

fn rust_response(fixture: &Fixture) -> Value {
    let results = fixture.cases.iter().map(normalize_case).collect::<Vec<_>>();
    ok_response(fixture.request_id, json!({ "results": results }))
}

fn normalize_case(case: &Case) -> Value {
    let diagnostic = PromptTemplateDiagnostic {
        r#type: PromptTemplateDiagnosticType::Warning,
        code: case.code,
        message: JsString::from(case.message.to_owned()),
        path: JsString::from(case.path.to_owned()),
    };
    json!({
        "name": case.name,
        "type": diagnostic.r#type.as_str(),
        "code": diagnostic.code.as_str(),
        "message": diagnostic.message,
        "path": diagnostic.path,
    })
}
