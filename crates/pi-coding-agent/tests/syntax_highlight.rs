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
                Arc::new(move |text: &str| format!("[{label}:{text}]")) as _,
            )
        })
        .collect::<HashMap<_, _>>()
}

#[test]
fn renders_highlighted_spans_with_the_provided_theme() {
    let rendered = render_highlighted_html(
        "<span class=\"hljs-keyword\">const</span> value",
        &theme(&[("keyword", "keyword")]),
    );
    assert_eq!(rendered, "[keyword:const] value");
}

#[test]
fn decodes_html_entities_emitted_by_highlight_js() {
    let rendered = render_highlighted_html(
        "&lt;tag attr=&quot;value&quot;&gt;&amp;#x41;&#65;&lt;/tag&gt;",
        &theme(&[]),
    );
    assert_eq!(rendered, "<tag attr=\"value\">&#x41;A</tag>");
}

#[test]
fn inherits_parent_formatting_for_unmapped_nested_scopes() {
    let interpolation = format!("${}{{x}}", "");
    let html = format!("<span class=\"hljs-string\">a<span class=\"hljs-subst\">{interpolation}</span>b</span>");
    let rendered = render_highlighted_html(&html, &theme(&[("string", "string")]));
    assert_eq!(rendered, format!("[string:a][string:{interpolation}][string:b]"));
}

#[test]
fn keeps_parent_formatting_across_unscoped_nested_spans() {
    let rendered = render_highlighted_html(
        "<span class=\"hljs-string\">a<span class=\"language-xml\">b</span>c</span>",
        &theme(&[("string", "string")]),
    );
    assert_eq!(rendered, "[string:a][string:b][string:c]");
}

#[test]
fn highlights_registered_languages_and_reports_the_rest_as_unavailable() {
    assert!(supports_language("rust"));
    assert!(supports_language("rs"));
    assert!(!supports_language("not-a-language"));
    assert!(supports_language("typescript"));

    let rendered = highlight(
        "fn main() {}",
        &HighlightOptions {
            language: Some("rust".to_owned()),
            ignore_illegals: Some(true),
            theme: theme(&[("keyword", "keyword"), ("title", "title")]),
            ..HighlightOptions::default()
        },
    );
    assert!(rendered.contains("[keyword:fn]"), "{rendered}");
    assert!(rendered.contains("[title:main]"), "{rendered}");
}

#[test]
fn unregistered_languages_fall_back_to_escaped_text() {
    let rendered = highlight(
        "<const> & \"x\"",
        &HighlightOptions {
            language: Some("not-a-language".to_owned()),
            ignore_illegals: Some(true),
            theme: theme(&[("default", "def")]),
            ..HighlightOptions::default()
        },
    );
    assert_eq!(rendered, "[def:<const> & \"x\"]");
}

#[test]
fn escapes_plain_text_without_a_language() {
    let rendered = highlight(
        "<script>&",
        &HighlightOptions {
            language: None,
            language_subset: Some(vec!["plaintext".to_owned()]),
            theme: theme(&[]),
            ..HighlightOptions::default()
        },
    );
    // `renderHighlightedHtml` decodes the entities highlight.js emitted.
    assert_eq!(rendered, "<script>&");
}

#[test]
fn renders_json_keywords_strings_and_numbers() {
    let rendered = highlight(
        "{\"key\": true, \"n\": 1}",
        &HighlightOptions {
            language: Some("json".to_owned()),
            ignore_illegals: Some(true),
            theme: theme(&[("attr", "attr"), ("literal", "literal"), ("number", "number")]),
            ..HighlightOptions::default()
        },
    );
    assert_eq!(rendered, "{[attr:\"key\"]: [literal:true], [attr:\"n\"]: [number:1]}");
}
