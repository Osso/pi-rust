//! Differential test: `session-selector-search` filtering, scoring, and sorting
//! against the pinned TypeScript module.

use crate::shared_oracle as oracle;

use oracle::TsOracle;
use pi_coding_agent::core::session_manager::SessionInfo;
use pi_coding_agent::modes::interactive::components::session_selector_search::{
    NameFilter, SortMode, filter_and_sort_sessions,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

const SOURCE_MODULE: &str = "packages/coding-agent/src/modes/interactive/components/session-selector-search.ts";

fn session(id: &str, modified: i64, first_message: &str) -> SessionInfo {
    SessionInfo {
        path: JsString::from(format!("/tmp/{id}.jsonl")),
        id: JsString::from(id.to_owned()),
        cwd: JsString::default(),
        name: None,
        is_archived: None,
        parent_session_path: None,
        created: 0.0,
        modified: modified as f64,
        message_count: 1,
        first_message: JsString::from(first_message.to_owned()),
    }
}

fn search_cases() -> Vec<Value> {
    let base = vec![
        session("candidate-a", 4, "could help arrange indexing results"),
        session("candidate-b", 3, "my computer chair broke"),
        session("candidate-c", 2, "could help arrange internal records"),
        session("candidate-d", 1, "replacement chair ordered"),
    ];
    let mut named = session("named", 9, "blueberry pie");
    named.name = Some(JsString::from("My Project"));
    let mut whitespace_named = session("whitespace", 8, "blueberry pie");
    whitespace_named.name = Some(JsString::from("   "));
    let mut empty = session("empty", 7, "blueberry pie");
    empty.message_count = 0;

    let mut cases = Vec::new();
    let mut push = |name: &str, sessions: &[SessionInfo], query: &str, sort: &str, filter: &str| {
        cases.push(json!({
            "name": name,
            "sessions": sessions.iter().map(|session| json!({
                "id": session.id,
                "name": session.name,
                "cwd": session.cwd,
                "messageCount": session.message_count,
                "firstMessage": session.first_message,
                "modified": session.modified,
            })).collect::<Vec<_>>(),
            "query": query,
            "sortMode": sort,
            "nameFilter": filter,
        }));
    };

    push(
        "not-later-message",
        &base,
        "re:obsolete later-message needle",
        "recent",
        "all",
    );
    push("phrase", &base, "\"node cve\"", "recent", "all");
    push("literal-chair", &base, "chair", "recent", "all");
    push("literal-chair-relevance", &base, "chair", "relevance", "all");
    push("complete-query", &base, "foo bar", "recent", "all");
    push("regex-word", &base, "re:\\bchair\\b", "recent", "all");
    push("regex-dot", &base, "re:ch.*r", "relevance", "all");
    push("regex-anchored", &base, "re:^candidate-b", "recent", "all");
    push("regex-lookahead", &base, "re:ch(?=air)", "recent", "all");
    push("regex-backreference", &base, "re:(a)\\1", "recent", "all");
    push("regex-invalid", &base, "re:(", "recent", "all");
    push("regex-empty", &base, "re:", "recent", "all");
    push("unicode-phrase", &base, "\"chère\"", "relevance", "all");
    push("unicode-score", &base, "\"😀\"", "relevance", "all");
    push("empty-query", &base, "   ", "recent", "all");
    push("mixed-quotes", &base, "\"chair\" replacement", "recent", "all");
    push("unclosed-quote", &base, "\"chair replacement", "relevance", "all");
    push(
        "named-filter",
        &[named.clone(), whitespace_named.clone(), empty.clone()],
        "",
        "recent",
        "named",
    );
    push(
        "named-filter-query",
        &[named.clone(), whitespace_named.clone(), empty.clone()],
        "blueberry",
        "relevance",
        "named",
    );
    push("threaded-sort", &base, "chair", "threaded", "all");
    push("no-match", &base, "zzzz", "relevance", "all");

    let mut unicode = vec![
        session("emoji", 5, "😀 brave start"),
        session("late", 4, "xxxx brave"),
        session("early", 3, "brave xxxx"),
        session("cjk", 2, "日本語 brave"),
    ];
    unicode[0].name = Some(JsString::from("Émile"));
    push("unicode-sessions", &unicode, "\"brave\"", "relevance", "all");
    push("unicode-name", &unicode, "émile", "recent", "all");
    cases
}

fn sort_mode(value: &str) -> SortMode {
    match value {
        "recent" => SortMode::Recent,
        "relevance" => SortMode::Relevance,
        _ => SortMode::Threaded,
    }
}

fn name_filter(value: &str) -> NameFilter {
    match value {
        "named" => NameFilter::Named,
        _ => NameFilter::All,
    }
}

fn rust_sessions(value: &Value) -> Vec<SessionInfo> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|session| SessionInfo {
            path: JsString::from(format!("/tmp/{}.jsonl", session["id"].as_str().unwrap())),
            id: JsString::from(session["id"].as_str().unwrap().to_owned()),
            cwd: JsString::from(session["cwd"].as_str().unwrap().to_owned()),
            name: session["name"].as_str().map(str::to_owned).map(JsString::from),
            is_archived: None,
            parent_session_path: None,
            created: 0.0,
            modified: session["modified"].as_f64().unwrap(),
            message_count: session["messageCount"].as_u64().unwrap() as usize,
            first_message: JsString::from(session["firstMessage"].as_str().unwrap().to_owned()),
        })
        .collect()
}

#[test]
fn session_search_matches_typescript() {
    let cases = search_cases();
    let mut oracle = TsOracle::spawn("coding-agent-session-search-oracle.ts", SOURCE_MODULE);
    let response = oracle.request(&json!({"id": 1, "operation": "search", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    let expected = response["output"]["results"].as_array().unwrap().clone();

    let actual: Vec<Value> = cases
        .iter()
        .map(|case| {
            let sessions = rust_sessions(&case["sessions"]);
            let result = filter_and_sort_sessions(
                &sessions,
                &JsString::from(case["query"].as_str().unwrap()),
                sort_mode(case["sortMode"].as_str().unwrap()),
                name_filter(case["nameFilter"].as_str().unwrap()),
            );
            json!({
                "name": case["name"],
                "ids": result.iter().map(|session| session.id.clone()).collect::<Vec<_>>(),
            })
        })
        .collect();

    for (index, (actual_case, expected_case)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual_case, expected_case,
            "search case {index} ({}) diverged",
            expected_case["name"]
        );
    }
}
