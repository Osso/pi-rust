use crate::shared_oracle as oracle;

use std::collections::HashMap;
use std::sync::Arc;

use pi_coding_agent::utils::syntax_highlight::{
    HighlightFormatter, HighlightOptions, HighlightTheme, highlight, render_highlighted_html, supports_language,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

const THEME: &[(&str, &str)] = &[
    ("keyword", "kw"),
    ("string", "str"),
    ("number", "num"),
    ("comment", "com"),
    ("title", "ttl"),
    ("attr", "atr"),
    ("literal", "lit"),
    ("built_in", "bi"),
    ("meta", "met"),
    ("type", "typ"),
    ("variable", "var"),
    ("function", "fn"),
    ("default", "def"),
];

enum Case {
    Highlight {
        name: String,
        code: String,
        language: Option<String>,
        ignore_illegals: bool,
        subset: Option<Vec<String>>,
    },
    Render {
        name: String,
        html: String,
    },
    Supports {
        name: String,
        language: String,
    },
}

#[test]
fn coding_agent_syntax_highlight_matches_the_pinned_typescript_source() {
    let cases = cases();
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-syntax-highlight-oracle.ts",
        "packages/coding-agent/src/utils/syntax-highlight.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_006,
        "operation": "coding_agent_syntax_highlight",
        "cases": cases.iter().map(request_case).collect::<Vec<_>>(),
    }));
    let actual = json!({"id": 27_006, "ok": true, "output": cases.iter().map(actual_case).collect::<Vec<_>>()});

    assert_eq!(expected["ok"], Value::Bool(true), "{}", expected);
    let expected_results = expected["output"].as_array().expect("oracle output array");
    let actual_results = actual["output"].as_array().expect("actual output array");
    assert_eq!(expected_results.len(), cases.len());
    let mut failures = Vec::new();
    for (index, (actual, expected)) in actual_results.iter().zip(expected_results).enumerate() {
        if actual != expected {
            failures.push(format!(
                "case {index} ({})\n  rust:   {actual}\n  oracle: {expected}",
                case_name(&cases[index])
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn case_name(case: &Case) -> &str {
    match case {
        Case::Highlight { name, .. } | Case::Render { name, .. } | Case::Supports { name, .. } => name,
    }
}

fn request_case(case: &Case) -> Value {
    let theme = THEME
        .iter()
        .map(|(scope, label)| json!([scope, label]))
        .collect::<Vec<_>>();
    match case {
        Case::Highlight {
            name,
            code,
            language,
            ignore_illegals,
            subset,
        } => json!({
            "kind": "highlight",
            "name": name,
            "code": code,
            "language": language,
            "ignoreIllegals": ignore_illegals,
            "subset": subset,
            "theme": theme,
        }),
        Case::Render { name, html } => json!({"kind": "render", "name": name, "html": html, "theme": theme}),
        Case::Supports { name, language } => json!({"kind": "supports", "name": name, "language": language}),
    }
}

fn theme() -> HighlightTheme {
    THEME
        .iter()
        .map(|(scope, label)| {
            let label = (*label).to_owned();
            let formatter: HighlightFormatter =
                Arc::new(move |text: &JsString| JsString::from(format!("<{label}>")) + text + format!("</{label}>"));
            ((*scope).to_owned(), formatter)
        })
        .collect::<HashMap<_, _>>()
}

fn actual_case(case: &Case) -> Value {
    match case {
        Case::Highlight {
            name,
            code,
            language,
            ignore_illegals,
            subset,
        } => {
            let rendered = highlight(
                &code.into(),
                &HighlightOptions {
                    language: language.as_ref().map(JsString::from),
                    ignore_illegals: Some(*ignore_illegals),
                    language_subset: subset.as_ref().map(|items| items.iter().map(JsString::from).collect()),
                    theme: theme(),
                },
            )
            .unwrap();
            json!({"name": name, "value": rendered})
        }
        Case::Render { name, html } => json!({"name": name, "value": render_highlighted_html(&html.into(), &theme())}),
        Case::Supports { name, language } => json!({"name": name, "value": supports_language(&language.into())}),
    }
}

fn snippet_case(name: &str, language: &str, code: &str, ignore_illegals: bool) -> Case {
    Case::Highlight {
        name: format!("{name}-{language}-{ignore_illegals}"),
        code: code.to_owned(),
        language: Some(language.to_owned()),
        ignore_illegals,
        subset: None,
    }
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    let snippets: &[(&str, &str)] = &[
        ("rust", RUST),
        ("json", JSON),
        ("bash", BASH),
        ("yaml", YAML),
        ("diff", DIFF),
        ("java", JAVA),
        ("plaintext", PLAINTEXT),
    ];
    for (language, code) in snippets {
        for ignore_illegals in [true, false] {
            cases.push(snippet_case("snippet", language, code, ignore_illegals));
        }
    }

    let grammars: Value = serde_json::from_str(GRAMMARS).expect("generated grammars must parse");
    for grammar in grammars["languages"].as_array().expect("languages") {
        let language = grammar["key"].as_str().expect("grammar key");
        cases.push(snippet_case("grammar", language, GENERIC_SNIPPET, false));
    }

    let tricky = [
        "unterminated \"string\nnext line",
        "/* unterminated comment\nmore",
        "a < b > c & d \"quoted\" 'single'",
        "let x = 1e10; let y = 0xFF; let z = 1_000;",
        "héllo 😀 \u{2028} ünïcode",
        "line\r\nwindows\r\nendings\r\n",
        "",
        "\n\n\n",
        "\\ escaped \\\\ double",
    ];
    for (index, code) in tricky.iter().enumerate() {
        for language in ["rust", "json", "bash", "yaml"] {
            cases.push(snippet_case(&format!("tricky-{index}"), language, code, true));
        }
    }

    for name in ["rust", "rs", "json", "Rust", "not-a-language", ""] {
        cases.push(Case::Supports {
            name: format!("supports-{name}"),
            language: name.to_owned(),
        });
    }

    let html_cases = [
        "<span class=\"hljs-keyword\">const</span> value",
        "&lt;tag attr=&quot;value&quot;&gt;&amp;#x41;&#65;&lt;/tag&gt;",
        "<span class=\"hljs-string\">a<span class=\"hljs-subst\">${x}</span>b</span>",
        "<span class=\"hljs-string\">a<span class=\"language-xml\">b</span>c</span>",
        "<span class='hljs-attr'>x</span><span class=\"hljs-attr\">y</span>",
        "<span class=hljs-attr>unquoted</span>",
        "<span class=\"hljs-a hljs-b\">multi</span>",
        "<span>no class</span><span class=\"\">empty</span>",
        "<span class=\"hljs-keyword\">unterminated",
        "<spanx class=\"hljs-keyword\">not a span</spanx>",
        "<span class=\"hljs-keyword\">a</span></span>",
        "<span class=\"hljs-keyword\">a</span><span class=\"hljs-keyword\">b</span>",
        "&amp; &unknown; &#x1F600; &#1114112; &;",
        "",
    ];
    for (index, html) in html_cases.iter().enumerate() {
        cases.push(Case::Render {
            name: format!("render-{index}"),
            html: (*html).to_owned(),
        });
    }

    cases.push(Case::Highlight {
        name: "auto-plaintext-subset".to_owned(),
        code: "<script>&".to_owned(),
        language: None,
        ignore_illegals: false,
        subset: Some(vec!["plaintext".to_owned()]),
    });
    cases.push(Case::Highlight {
        name: "auto-json-subset".to_owned(),
        code: JSON.to_owned(),
        language: None,
        ignore_illegals: false,
        subset: Some(vec!["json".to_owned(), "rust".to_owned()]),
    });
    cases.push(Case::Highlight {
        name: "auto-all".to_owned(),
        code: RUST.to_owned(),
        language: None,
        ignore_illegals: false,
        subset: None,
    });

    cases
}

const GRAMMARS: &str = include_str!("../../pi-coding-agent-base/src/utils/syntax_highlight/grammars.json");

/// Multi-construct snippet exercised against every generated grammar.
const GENERIC_SNIPPET: &str = r##"/* block comment */
// line comment
# hash comment
-- dash comment
; semicolon comment
def function_name(arg1, arg2):
    value = 42
    text = "double \"quoted\" string"
    other = 'single \'quoted\' string'
    if value >= 3.14 and not arg1:
        return {"key": [1, 2, 3], "nested": true}
class Example extends Base {
  method() { return this.value + 1; }
}
<p class="tag">text &amp; more</p>
@decorator
$x = %y + &z
"##;

const RUST: &str = r##"use std::collections::HashMap;

/// Doc comment with `code` and "quotes".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counter<'a> {
    map: HashMap<&'a str, u64>,
}

impl<'a> Counter<'a> {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    pub fn bump(&mut self, key: &'a str) -> u64 {
        let entry = self.map.entry(key).or_insert(0);
        *entry += 1;
        *entry
    }
}

fn main() {
    let mut counter = Counter::new();
    let value = counter.bump("alpha");
    println!("value = {value} {:#x} {:?}", 255, b"bytes");
    let raw = r#"raw "string" here"#;
    let byte = b'\n';
    let _ = (raw, byte, 1_000.5e-3, 0o755, 0b1010);
    // line comment with TODO: fix
    /* block comment */
    if value > 0 && !raw.is_empty() {
        return;
    }
}
"##;

const JSON: &str = r#"{
  "name": "pi-rust",
  "version": "0.0.0",
  "private": true,
  "keywords": ["alpha", "beta"],
  "nested": { "n": -1.5e10, "flag": true, "none": null },
  "escaped": "line\nbreak \"quote\" \\ end"
}
"#;

const BASH: &str = r#"#!/usr/bin/env bash
set -euo pipefail

readonly NAME="world"
count=0

greet() {
  local target="${1:-$NAME}"
  echo "hello, ${target}!" >&2
  count=$((count + 1))
}

for arg in "$@"; do
  case "$arg" in
    --verbose|-v) set -x ;;
    --*) echo "unknown ${arg}" ;;
    *) greet "$arg" ;;
  esac
done

if [[ -f /etc/passwd && ! -d /tmp ]]; then
  cat /etc/passwd | grep -v '^#' | head -n 3
fi
"#;

const YAML: &str = r#"# comment
name: pi-rust
version: 0.0.0
list:
  - one
  - two
nested:
  key: "quoted value"
  number: 1.5
  flag: true
  nothing: null
multi: |
  line one
  line two
folded: >
  folded text
anchors:
  base: &base
    a: 1
  derived:
    <<: *base
"#;

const DIFF: &str = r#"diff --git a/file.txt b/file.txt
index 83db48f..bf269f4 100644
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,4 @@
 unchanged
-removed line
+added line
+another added
 context
"#;

const JAVA: &str = r#"package com.example;

import java.util.List;

/** Doc comment. */
public final class Greeter<T extends Comparable<T>> implements Runnable {
    private final List<T> values;

    public Greeter(List<T> values) {
        this.values = values;
    }

    @Override
    public void run() {
        for (T value : values) {
            System.out.println("value: " + value + 'x');
        }
    }
}
"#;

const PLAINTEXT: &str = "just some plain text\nwith <angle> & \"quotes\"\nand 'apostrophes'\n";
