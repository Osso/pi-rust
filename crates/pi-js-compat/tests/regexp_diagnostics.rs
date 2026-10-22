#[path = "../../pi-tui/tests/support/oracle.rs"]
mod oracle;
use pi_js_compat::{js_json::JsString, js_regex::JsRegex};
use serde_json::{Value, json};

fn cases() -> Vec<(String, String)> {
    let mut cases: Vec<_> = [
        ("(", "i"),
        (" )", "i"),
        ("(()))(", "i"),
        ("[", "i"),
        ("[\\", "i"),
        ("\\", "i"),
        ("[z-a]", "i"),
        ("*a", "i"),
        ("+", "i"),
        ("?", "i"),
        ("{1}", "i"),
        ("a{2,1}", "i"),
        ("a{", "u"),
        ("a{2,1}", "u"),
        ("{", "u"),
        ("}", "u"),
        ("]", "u"),
        ("a**", "i"),
        ("^*", "i"),
        ("(?<=a)+", "i"),
        ("(?=a)+", "u"),
        ("(?x:a)", "i"),
        ("(?ii:a)", "i"),
        ("(?i--m:a)", "i"),
        ("(?-:a)", "i"),
        ("(?i", "i"),
        ("(?", "i"),
        ("(?<1>a)", "i"),
        ("(?<a>a)(?<a>b)", "i"),
        ("(?<a>a)\\k<missing>", "i"),
        ("(?<a>a)\\k", "i"),
        ("\\k<x>", "u"),
        ("\\u{GG}", "u"),
        ("\\xG1", "u"),
        ("\\8", "u"),
        ("\\1", "u"),
        ("\\q", "u"),
        ("[\\c]", "u"),
        ("[\\d-a]", "u"),
        ("\\pX", "u"),
        ("\\p{}", "u"),
        ("\\p{Invalid}", "u"),
        ("\\p{Invalid=Other}", "u"),
        ("[\\p{RGI_Emoji}]", "v"),
        ("[", "v"),
        ("[a", "v"),
        ("[a&&]", "v"),
        ("[a--]", "v"),
        ("[a&&b-c]", "v"),
        ("[a--b&&c]", "v"),
        ("[a&&bc]", "v"),
        ("[a--bc]", "v"),
        ("[z-a]", "v"),
        ("[\\d-a]", "v"),
        ("[\\", "v"),
        ("[\\q]", "v"),
        ("[\\q{ab", "v"),
        ("[() ]", "v"),
        ("[!!]", "v"),
        ("[a-", "v"),
        ("[^\\p{RGI_Emoji}]", "v"),
        ("[\\P{RGI_Emoji}]", "v"),
        ("[\\q{ab}--]", "v"),
    ]
    .into_iter()
    .map(|(pattern, flags)| (pattern.to_owned(), flags.to_owned()))
    .collect();
    cases.push(("(".repeat(257) + &")".repeat(257), "i".into()));
    cases.push(("()".repeat(65536), "i".into()));
    cases.push(("a*".repeat(65536), "i".into()));
    cases
}

#[test]
fn regress_parser_error_categories_match_pinned_v8_diagnostics() {
    let cases = cases();
    let mut oracle = oracle::TsOracle::spawn("regexp-diagnostics-oracle.mjs", "packages/tui/src");
    let expected=oracle.request(&json!({"id":1,"cases":cases.iter().map(|(pattern,flags)|json!({"units":pattern.encode_utf16().collect::<Vec<_>>(),"flags":flags})).collect::<Vec<_>>()}));
    assert_eq!(expected["ok"], true, "{expected}");
    let mut corpus = Vec::new();
    let mut mismatches = 0;
    for ((pattern, flags), node) in cases.iter().zip(expected["results"].as_array().unwrap()) {
        let (native, category) = match JsRegex::with_flags(pattern.as_str(), flags) {
            Ok(_) => (json!({"accepted":true}), Value::Null),
            Err(error) => {
                let text = error.to_string();
                (
                    json!({"accepted":false,"name":"SyntaxError","units":text.encode_utf16().collect::<Vec<_>>()}),
                    json!(text),
                )
            }
        };
        let matches = native == *node;
        if !matches {
            mismatches += 1;
        }
        let node_message = node["units"]
            .as_array()
            .map(|units| JsString::from_utf16(units.iter().map(|unit| unit.as_u64().unwrap() as u16).collect()));
        let node_scalar = node_message
            .as_ref()
            .and_then(|message| String::from_utf16(message.units()).ok());
        corpus.push(json!({"pattern":pattern,"flags":flags,"regressCategory":category,"native":native,"node":node,"nodeMessage":node_scalar,"matches":matches}));
    }
    let output = json!({"node":expected["node"],"v8":expected["v8"],"executable":expected["executable"],"cases":corpus,"mismatches":mismatches});
    println!("REGEXP_CORPUS {}", output);
    assert_eq!(
        mismatches, 0,
        "diagnostic/acceptance mismatches; see regexp-diagnostic-corpus.json"
    );
}
