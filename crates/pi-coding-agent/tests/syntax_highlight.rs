use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::sync::Arc;

use pi_coding_agent::utils::syntax_highlight::{
    HighlightOptions, HighlightTheme, highlight, render_highlighted_html, supports_language,
};

fn theme(pairs: &[(&str, &str)]) -> HighlightTheme {
    pairs
        .iter()
        .map(|(scope, label)| {
            let label = (*label).to_owned();
            (
                (*scope).to_owned(),
                Arc::new(move |text: &JsString| JsString::from(format!("[{label}:")) + text + "]") as _,
            )
        })
        .collect::<HashMap<_, _>>()
}

#[test]
fn renders_highlighted_spans_with_the_provided_theme() {
    let rendered = render_highlighted_html(
        &"<span class=\"hljs-keyword\">const</span> value".into(),
        &theme(&[("keyword", "keyword")]),
    );
    assert_eq!(rendered, "[keyword:const] value");
}

#[test]
fn decodes_html_entities_emitted_by_highlight_js() {
    let rendered = render_highlighted_html(
        &"&lt;tag attr=&quot;value&quot;&gt;&amp;#x41;&#65;&lt;/tag&gt;".into(),
        &theme(&[]),
    );
    assert_eq!(rendered, "<tag attr=\"value\">&#x41;A</tag>");
}

#[test]
fn inherits_parent_formatting_for_unmapped_nested_scopes() {
    let interpolation = format!("${}{{x}}", "");
    let html = format!("<span class=\"hljs-string\">a<span class=\"hljs-subst\">{interpolation}</span>b</span>");
    let rendered = render_highlighted_html(&html.into(), &theme(&[("string", "string")]));
    assert_eq!(rendered, format!("[string:a][string:{interpolation}][string:b]"));
}

#[test]
fn keeps_parent_formatting_across_unscoped_nested_spans() {
    let rendered = render_highlighted_html(
        &"<span class=\"hljs-string\">a<span class=\"language-xml\">b</span>c</span>".into(),
        &theme(&[("string", "string")]),
    );
    assert_eq!(rendered, "[string:a][string:b][string:c]");
}

#[test]
fn highlights_registered_languages_and_reports_the_rest_as_unavailable() {
    assert!(supports_language(&"rust".into()));
    assert!(supports_language(&"rs".into()));
    assert!(!supports_language(&"not-a-language".into()));
    assert!(supports_language(&"typescript".into()));

    let rendered = highlight(
        &"fn main() {}".into(),
        &HighlightOptions {
            language: Some("rust".into()),
            ignore_illegals: Some(true),
            theme: theme(&[("keyword", "keyword"), ("title", "title")]),
            ..HighlightOptions::default()
        },
    )
    .unwrap();
    assert!(rendered.contains("[keyword:fn]"), "{rendered:?}");
    assert!(rendered.contains("[title:main]"), "{rendered:?}");
}

#[test]
fn unregistered_languages_return_the_frozen_error() {
    let rendered = highlight(
        &"<const> & \"x\"".into(),
        &HighlightOptions {
            language: Some("not-a-language".into()),
            ignore_illegals: Some(true),
            theme: theme(&[("default", "def")]),
            ..HighlightOptions::default()
        },
    );
    assert_eq!(rendered.unwrap_err().message, "Unknown language: \"not-a-language\"");
}

#[test]
fn escapes_plain_text_without_a_language() {
    let rendered = highlight(
        &"<script>&".into(),
        &HighlightOptions {
            language: None,
            language_subset: Some(vec!["plaintext".into()]),
            theme: theme(&[]),
            ..HighlightOptions::default()
        },
    )
    .unwrap();
    // `renderHighlightedHtml` decodes the entities highlight.js emitted.
    assert_eq!(rendered, "<script>&");
}

#[test]
fn renders_json_keywords_strings_and_numbers() {
    let rendered = highlight(
        &"{\"key\": true, \"n\": 1}".into(),
        &HighlightOptions {
            language: Some("json".into()),
            ignore_illegals: Some(true),
            theme: theme(&[("attr", "attr"), ("literal", "literal"), ("number", "number")]),
            ..HighlightOptions::default()
        },
    )
    .unwrap();
    assert_eq!(rendered, "{[attr:\"key\"]: [literal:true], [attr:\"n\"]: [number:1]}");
}
