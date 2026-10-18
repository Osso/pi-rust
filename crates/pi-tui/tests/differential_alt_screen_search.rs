//! Differential tests for `alt-screen-search.ts` against the pinned TypeScript source.

#[path = "support/oracle.rs"]
mod oracle;

use oracle::TsOracle;
use pi_tui::alt_screen_search::{
    AltScreenSearchComponent, AltScreenSearchMatch, find_alt_screen_search_matches, get_alt_screen_search_match_key,
};
use pi_tui::tui::{Component, Focusable};
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";

fn matches_json(search_matches: &[AltScreenSearchMatch]) -> Value {
    Value::Array(
        search_matches
            .iter()
            .map(|search_match| {
                json!({
                    "segments": search_match
                        .segments
                        .iter()
                        .map(|segment| json!({
                            "row": segment.row,
                            "startCol": segment.start_col,
                            "endCol": segment.end_col,
                        }))
                        .collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

#[test]
fn search_matches_match_typescript() {
    let cases = vec![
        json!({"lines": ["alpha QUICK", "brown fox"], "query": "quick brown"}),
        json!({"lines": ["alpha"], "query": ""}),
        json!({"lines": ["alpha"], "query": "   \t "}),
        json!({"lines": ["NEEDLE   one"], "query": " needle one "}),
        json!({"lines": ["  alpha", "  beta"], "query": "alpha beta"}),
        json!({"lines": ["one two one", "one"], "query": "one"}),
        json!({"lines": ["\u{1b}[1malpha\u{1b}[0m beta"], "query": "alpha beta"}),
        json!({"lines": ["A界🙂e\u{301}Z"], "query": "界🙂"}),
        json!({"lines": ["A界🙂e\u{301}Z"], "query": "E\u{301}Z"}),
        json!({"lines": ["MiXeD Case", "mixed"], "query": "mixed"}),
        json!({"lines": ["first", "second", "third"], "query": "second third"}),
        json!({"lines": ["needle", "middle", "needle"], "query": "needle"}),
        json!({"lines": ["\u{1b}]133;A\u{7}prompt", "output"], "query": "prompt output"}),
        json!({"lines": ["a.b*c", "a.b*c"], "query": "a.b*c"}),
        json!({"lines": ["naïve café"], "query": "CAFÉ"}),
    ];
    let mut oracle = TsOracle::spawn("tui-alt-screen-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({
        "id": 1,
        "operation": "search_matches",
        "cases": cases,
    }));
    assert_eq!(response["ok"], Value::Bool(true), "{response}");

    for (index, case) in cases.iter().enumerate() {
        let lines: Vec<String> = case["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| line.as_str().unwrap().to_string())
            .collect();
        let query = case["query"].as_str().unwrap();
        let actual = find_alt_screen_search_matches(&lines, query);
        let expected = &response["output"]["results"][index];
        assert_eq!(matches_json(&actual), expected["matches"], "{case}");
        let keys: Vec<String> = actual.iter().map(get_alt_screen_search_match_key).collect();
        assert_eq!(json!(keys), expected["keys"], "{case}");
    }
}

#[test]
fn search_component_matches_typescript() {
    let steps = json!([
        {"kind": "render"},
        {"kind": "focus", "focused": true},
        {"kind": "render"},
        {"kind": "input", "data": "n"},
        {"kind": "input", "data": "e"},
        {"kind": "input", "data": "e"},
        {"kind": "render"},
        {"kind": "result", "index": 1, "count": 3},
        {"kind": "render"},
        {"kind": "result", "index": -1, "count": 0},
        {"kind": "input", "data": "\u{1b}[D"},
        {"kind": "input", "data": "\u{7f}"},
        {"kind": "render"},
        {"kind": "focus", "focused": false},
        {"kind": "render"},
    ]);
    let width = 24_usize;
    let mut oracle = TsOracle::spawn("tui-alt-screen-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({
        "id": 2,
        "operation": "search_component",
        "width": width,
        "steps": steps,
    }));
    assert_eq!(response["ok"], Value::Bool(true), "{response}");

    let queries: std::rc::Rc<std::cell::RefCell<Vec<String>>> = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = std::rc::Rc::clone(&queries);
    let mut component = AltScreenSearchComponent::new(std::rc::Rc::new(move |query: &str| {
        sink.borrow_mut().push(query.to_string());
    }));
    let mut renders: Vec<Vec<String>> = Vec::new();
    for step in steps.as_array().unwrap() {
        match step["kind"].as_str().unwrap() {
            "render" => renders.push(component.render(width)),
            "input" => component.handle_input(step["data"].as_str().unwrap()),
            "focus" => component.set_focused(step["focused"].as_bool().unwrap()),
            "result" => component.set_result(
                step["index"].as_i64().unwrap(),
                step["count"].as_u64().unwrap() as usize,
            ),
            other => panic!("unsupported step {other}"),
        }
    }

    assert_eq!(json!(renders), response["output"]["renders"]);
    assert_eq!(json!(*queries.borrow()), response["output"]["queries"]);
}
