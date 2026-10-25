use crate::shared_oracle as oracle;

use pi_coding_agent::utils::html::{decode_html_entity, decode_html_entity_at};
use pi_coding_agent::utils::syntax_highlight::{
    HighlightFormatter, HighlightOptions, HighlightTheme, highlight, highlight_html, render_highlighted_html,
    supports_language,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use std::sync::Arc;

fn theme() -> HighlightTheme {
    [
        "keyword", "string", "number", "comment", "title", "attr", "literal", "built_in", "meta", "type", "variable",
        "function", "default",
    ]
    .into_iter()
    .map(|scope| {
        let formatter: HighlightFormatter =
            Arc::new(move |text| JsString::from(format!("<{scope}>")) + text + format!("</{scope}>"));
        (scope.to_owned(), formatter)
    })
    .collect()
}

fn compare(
    oracle: &mut oracle::TsOracle,
    kind: &str,
    code: &JsString,
    language: Option<&JsString>,
    subset: Option<Vec<JsString>>,
) {
    let expected = oracle.request(&json!({"cases": [{"kind": kind, "code": code.units(), "language": language.map(JsString::units), "subset": subset.as_ref().map(|items| items.iter().map(JsString::units).collect::<Vec<_>>())}]}));
    let options = HighlightOptions {
        language: language.cloned(),
        ignore_illegals: Some(true),
        language_subset: subset,
        theme: theme(),
    };
    let actual = match kind {
        "supports" => Value::Bool(supports_language(code)),
        "render" => json!(render_highlighted_html(code, &options.theme).units()),
        _ => match if kind == "raw" {
            highlight_html(code, &options)
        } else {
            highlight(code, &options)
        } {
            Ok(output) => json!(output.units()),
            Err(error) => json!({"error": error.message.units()}),
        },
    };
    assert_eq!(actual, expected[0], "kind={kind}; language={language:?}; code={code:?}");
}

#[test]
fn frozen_alias_code_preserves_unpaired_utf16() {
    let code = JsString::from("const text = \"")
        + JsString::from_utf16(vec![0xd800])
        + "\"; // "
        + JsString::from_utf16(vec![0xdc00]);
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-highlight-oracle.mjs",
        "packages/coding-agent/src/utils/syntax-highlight.ts",
    );
    for language in [
        "js",
        "JS",
        "ts",
        "rs",
        "python",
        "sh",
        "xml",
        "json",
        "c",
        "cpp",
        "ruby",
        "swift",
        "scala",
        "java",
        "css",
        "yaml",
        "perl",
        "sql",
        "plaintext",
    ] {
        for kind in ["raw", "highlight"] {
            compare(&mut oracle, kind, &code, Some(&language.into()), None);
        }
    }
    for raw in [
        vec![0xd800],
        vec![0xdc00],
        vec![0xd83d, 0xde00],
        vec![0xfffd],
        vec![0xd800, 0xdc00, 0xdc00],
        vec![0xd800, 97, 0xdc00],
    ] {
        let raw = JsString::from_utf16(raw);
        for code in [
            raw.clone(),
            JsString::from("/* ") + &raw + " */",
            JsString::from("\"") + &raw + "\"",
            JsString::from("<a>") + &raw + "</a>",
            JsString::from("const ") + &raw + " = 1;",
            JsString::from("# ") + &raw,
        ] {
            for language in ["js", "rs", "xml", "python", "swift"] {
                compare(&mut oracle, "raw", &code, Some(&language.into()), None);
            }
        }
    }
}

#[test]
fn frozen_html_entity_api_uses_unit_offsets_and_retains_surrogates() {
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-highlight-oracle.mjs",
        "packages/coding-agent/src/utils/syntax-highlight.ts",
    );
    for entity in [
        JsString::from("#xd800"),
        "#56320".into(),
        "#xD800junk".into(),
        "#\u{feff}+0".into(),
        "#x-0".into(),
        JsString::from("#65") + JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xd800]),
    ] {
        let expected = oracle.request(&json!({"cases": [{"kind":"entity", "code":entity.units(), "language": null}]}));
        let actual = decode_html_entity(&entity).map(|text| text.units().to_vec());
        assert_eq!(json!(actual), expected[0]);
    }
    for (html, index) in [
        (JsString::from_utf16(vec![0xd800, 0xdc00, 0xd800]) + "&#xd800;", 3),
        (JsString::from("xamp;"), 0),
        (JsString::from("&#00000000000065;"), 0),
        (JsString::from("&#000000000000065;"), 0),
    ] {
        let expected = oracle
            .request(&json!({"cases": [{"kind":"entity_at", "code":html.units(), "index":index, "language": null}]}));
        let actual = decode_html_entity_at(&html, index)
            .map(|decoded| json!({"text":decoded.text.units(), "length":decoded.length}));
        assert_eq!(json!(actual), expected[0]);
    }
}

#[test]
fn frozen_html_entities_scopes_and_unknown_language_errors() {
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-highlight-oracle.mjs",
        "packages/coding-agent/src/utils/syntax-highlight.ts",
    );
    let raw = JsString::from_utf16(vec![0xd800]);
    for html in [
        raw.clone(),
        JsString::from("<span class=\"hljs-string\">") + &raw + "&#55296;&#xDC00;&#x1f600;&amp;</span>",
        JsString::from("<span class='hljs-") + &raw + "'>raw</span>",
        JsString::from("<span class='hljs-constructor'>") + &raw + "</span>",
        JsString::from("&#-0;&#x+0xD800;&#xD800oops;&#x110000;&#999999999999999999999;&#x;"),
        JsString::from("<span\u{feff}class = 'hljs-keyword.extra'>x</span>"),
    ] {
        compare(&mut oracle, "render", &html, None, None);
    }
    for language in [
        raw.clone(),
        JsString::from("js") + &raw,
        JsString::from("unknown"),
        "".into(),
    ] {
        compare(&mut oracle, "supports", &language, None, None);
        compare(
            &mut oracle,
            "highlight",
            &raw,
            Some(&language),
            Some(vec!["plaintext".into()]),
        );
    }
    compare(
        &mut oracle,
        "highlight",
        &raw,
        None,
        Some(vec!["js".into(), raw.clone(), "plaintext".into()]),
    );
}
