//! Differential test: `session-selector-search` filtering, scoring, and sorting
//! against the pinned TypeScript module.

#[path = "support/oracle.rs"]
mod oracle;

use oracle::TsOracle;
use pi_coding_agent::core::session_manager::SessionInfo;
use pi_coding_agent::modes::interactive::components::session_selector_search::{
    NameFilter, SortMode, filter_and_sort_sessions,
};
use serde_json::{Value, json};

const SOURCE_MODULE: &str = "packages/coding-agent/src/modes/interactive/components/session-selector-search.ts";

fn session(id: &str, modified: i64, all_messages_text: &str) -> SessionInfo {
    SessionInfo {
        path: format!("/tmp/{id}.jsonl"),
        id: id.to_owned(),
        cwd: String::new(),
        name: None,
        is_archived: None,
        parent_session_path: None,
        created: 0,
        modified,
        message_count: 1,
        first_message: "(no messages)".to_owned(),
        all_messages_text: all_messages_text.to_owned(),
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
    named.name = Some("My Project".to_owned());
    let mut whitespace_named = session("whitespace", 8, "blueberry pie");
    whitespace_named.name = Some("   ".to_owned());
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
                "allMessagesText": session.all_messages_text,
                "modified": session.modified,
            })).collect::<Vec<_>>(),
            "query": query,
            "sortMode": sort,
            "nameFilter": filter,
        }));
    };

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
    unicode[0].name = Some("Émile".to_owned());
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
            path: format!("/tmp/{}.jsonl", session["id"].as_str().unwrap()),
            id: session["id"].as_str().unwrap().to_owned(),
            cwd: session["cwd"].as_str().unwrap().to_owned(),
            name: session["name"].as_str().map(str::to_owned),
            is_archived: None,
            parent_session_path: None,
            created: 0,
            modified: session["modified"].as_i64().unwrap(),
            message_count: session["messageCount"].as_u64().unwrap() as usize,
            first_message: "(no messages)".to_owned(),
            all_messages_text: session["allMessagesText"].as_str().unwrap().to_owned(),
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
                case["query"].as_str().unwrap(),
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
