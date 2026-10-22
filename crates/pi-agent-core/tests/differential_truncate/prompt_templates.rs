use pi_agent_core::harness::PromptTemplate;
use pi_agent_core::harness::prompt_templates::{
    format_prompt_template_invocation, parse_command_args, substitute_args,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_046;
const EXPECTED_FIXTURE_COUNT: usize = 15;

struct Fixture {
    request_id: u64,
    name: &'static str,
    args_string: &'static str,
    args: &'static [&'static str],
    content: &'static str,
    template_name: &'static str,
    template_description: Option<&'static str>,
    template_content: Option<&'static str>,
    omit_format_args: bool,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "prompt-template mismatch: {}", fixture.name);
    }
}

fn assert_request_ids(fixtures: &[Fixture]) {
    for (offset, fixture) in fixtures.iter().enumerate() {
        let expected = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        assert_eq!(fixture.request_id, expected, "fixture IDs must remain contiguous");
    }
}

fn request(fixture: &Fixture) -> Value {
    let mut request = json!({
        "id": fixture.request_id,
        "operation": "prompt_template_helpers",
        "argsString": fixture.args_string,
        "args": fixture.args,
        "content": fixture.content,
        "templateName": fixture.template_name,
        "omitFormatArgs": fixture.omit_format_args,
    });
    if let Some(description) = fixture.template_description {
        request["templateDescription"] = json!(description);
    }
    if let Some(content) = fixture.template_content {
        request["templateContent"] = json!(content);
    }
    request
}

fn rust_response(fixture: &Fixture) -> Value {
    let args = owned_args(fixture.args);
    let template_content = fixture.template_content.unwrap_or(fixture.content);
    let template = PromptTemplate {
        name: JsString::from(fixture.template_name.to_owned()),
        description: fixture.template_description.map(JsString::from),
        content: template_content.into(),
    };
    let format_args = if fixture.omit_format_args {
        Vec::new()
    } else {
        args.clone()
    };
    ok_response(
        fixture.request_id,
        json!({
            "parsedArgs": parse_command_args(fixture.args_string),
            "substituted": substitute_args(fixture.content, &args),
            "formatted": format_prompt_template_invocation(&template, &format_args),
        }),
    )
}

fn owned_args(values: &[&str]) -> Vec<JsString> {
    values.iter().copied().map(JsString::from).collect()
}

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            request_id: 13_046,
            name: "empty parser and omitted format arguments",
            args_string: "",
            args: &[],
            content: "$1 $@",
            template_name: "empty",
            template_description: None,
            template_content: None,
            omit_format_args: true,
        },
        Fixture {
            request_id: 13_047,
            name: "quotes adjacent fragments and unmatched quote",
            args_string: "  'a b'\t\"c d\" pre'quoted'\"text\" 'opposite\"quote' \"unterminated",
            args: &["one", "two", "three"],
            content: "$1|$2|$3",
            template_name: "quotes",
            template_description: Some("quoted arguments"),
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_048,
            name: "non-delimiter whitespace and literal backslashes",
            args_string: "a\nb\rc\u{00a0}d\\ e '' \"\"",
            args: &["a\nb\rc\u{00a0}d\\", "e"],
            content: "$ARGUMENTS",
            template_name: "whitespace",
            template_description: None,
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_049,
            name: "positional indices and malformed placeholders",
            args_string: "ignored",
            args: &["one", "two"],
            content: "$01/$0002/$0/$999999999999999999999/$-1/$ 1/$x/$$1",
            template_name: "positional",
            template_description: None,
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_050,
            name: "slice indices lengths and invalid forms",
            args_string: "ignored",
            args: &["one", "two", "three"],
            content: "${@:0}|${@:01:0}|${@:0002:003}|${@:999999999999999999999}|${@:1:999999999999999999999}|${@:1:}|${@:-1}|${@:1:-1}|${@:1.5}|${@:1:2}x",
            template_name: "slices",
            template_description: None,
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_051,
            name: "callback replacements are not rescanned in their pass",
            args_string: "ignored",
            args: &["$1", "${@:2}", "tail"],
            content: "$1|${@:2:1}",
            template_name: "callback",
            template_description: None,
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_052,
            name: "aggregate placeholders and empty argument joining",
            args_string: "ignored",
            args: &["one", "", "two"],
            content: "x$ARGUMENTS|$@|$@|$ARGUMENTS1|$@@",
            template_name: "aggregates",
            template_description: None,
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_053,
            name: "aggregate replacement tokens in both passes",
            args_string: "ignored",
            args: &["$$|$&|$1|$<name>"],
            content: "$ARGUMENTS|$@",
            template_name: "replacement tokens",
            template_description: None,
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_054,
            name: "aggregate prefix expansion across matches",
            args_string: "ignored",
            args: &["$`"],
            content: "x$ARGUMENTS y$ARGUMENTS z",
            template_name: "prefix",
            template_description: None,
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_055,
            name: "aggregate suffix expansion across matches",
            args_string: "ignored",
            args: &["$'"],
            content: "x$@ y$@ z",
            template_name: "suffix",
            template_description: None,
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_056,
            name: "cross-pass insertion and final pass behavior",
            args_string: "ignored",
            args: &["$@", "x"],
            content: "$1|${@:1}|$ARGUMENTS",
            template_name: "cross-pass",
            template_description: None,
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_057,
            name: "embedded aggregate and slice matches",
            args_string: "ignored",
            args: &["a", "b"],
            content: "${@:1:2}x|$ARGUMENTS1|$@@|$$1|X$ARGUMENTSY",
            template_name: "embedded",
            template_description: None,
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_058,
            name: "unicode combining nul and empty arguments",
            args_string: "😀 e\u{301}\0,!?'quoted value'",
            args: &["😀\0x", "e\u{301}", ""],
            content: "[$1]|[$ARGUMENTS]|[$@]",
            template_name: "strings",
            template_description: None,
            template_content: None,
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_059,
            name: "wrapper uses template content and provided arguments",
            args_string: "ignored",
            args: &["value"],
            content: "unused $1",
            template_name: "ignored name",
            template_description: Some("ignored description"),
            template_content: Some("Wrap $1"),
            omit_format_args: false,
        },
        Fixture {
            request_id: 13_060,
            name: "wrapper omitted arguments preserve content",
            args_string: "ignored",
            args: &["value"],
            content: "unused",
            template_name: "ignored name",
            template_description: Some("ignored description"),
            template_content: Some("😀\0$1"),
            omit_format_args: true,
        },
    ]
}
