#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_manager::SessionInfo;
use pi_coding_agent::modes::interactive::components::session_selector_search::{
    NameFilter, SortMode, filter_and_sort_sessions, match_session, parse_search_query,
};
use serde_json::{Value, json};

fn session(id: &str, modified_ms: i64, all_messages_text: &str) -> (Value, SessionInfo) {
    let request = json!({
        "id": id,
        "modifiedMs": modified_ms,
        "allMessagesText": all_messages_text,
    });
    let info = SessionInfo {
        path: format!("/tmp/{id}.jsonl"),
        id: id.to_string(),
        cwd: String::new(),
        name: None,
        is_archived: None,
        parent_session_path: None,
        created: 0,
        modified: modified_ms,
        message_count: 1,
        first_message: "(no messages)".to_string(),
        all_messages_text: all_messages_text.to_string(),
    };
    (request, info)
}

#[test]
fn session_selector_search_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("session-selector-search-oracle.ts", "packages/coding-agent/src");

    let (request_sessions, rust_sessions) = corpus();
    for (query, sort_mode) in [
        ("", SortMode::Recent),
        ("\"node cve\"", SortMode::Recent),
        ("re:\\bbrave\\b", SortMode::Recent),
        ("chair", SortMode::Recent),
        ("chair", SortMode::Relevance),
        ("foo bar", SortMode::Recent),
        ("foo bar", SortMode::Relevance),
        ("\"alpha\" chair", SortMode::Recent),
        ("re:(", SortMode::Recent),
    ] {
        let sort_name = match sort_mode {
            SortMode::Threaded => "threaded",
            SortMode::Recent => "recent",
            SortMode::Relevance => "relevance",
        };
        let expected = oracle.request(&json!({
            "id": 1,
            "operation": "filter",
            "sessions": request_sessions,
            "query": query,
            "sortMode": sort_name,
            "nameFilter": "all",
        }));
        let result = filter_and_sort_sessions(&rust_sessions, query, sort_mode, NameFilter::All);
        let actual: Vec<&str> = result.iter().map(|session| session.id.as_str()).collect();
        assert_eq!(
            json!(actual),
            expected["output"],
            "filter mismatch for {query} / {sort_name}"
        );
    }

    let queries = [
        "",
        "re:",
        "re:(",
        "re:brave",
        "\"alpha\" chair",
        "unclosed \"quote",
        "a  b",
        "  spaced  ",
    ];
    let expected = oracle.request(&json!({
        "id": 2,
        "operation": "parse",
        "queries": queries,
    }));
    let actual: Vec<Value> = queries
        .iter()
        .map(|query| {
            let parsed = parse_search_query(query);
            json!({
                "mode": match parsed.mode {
                    pi_coding_agent::modes::interactive::components::session_selector_search::SearchMode::Tokens => "tokens",
                    pi_coding_agent::modes::interactive::components::session_selector_search::SearchMode::Regex => "regex",
                },
                "tokens": parsed.tokens.iter().map(|token| json!({
                    "kind": match token.kind {
                        pi_coding_agent::modes::interactive::components::session_selector_search::TokenKind::Fuzzy => "fuzzy",
                        pi_coding_agent::modes::interactive::components::session_selector_search::TokenKind::Phrase => "phrase",
                    },
                    "value": token.value,
                })).collect::<Vec<_>>(),
                "hasRegex": parsed.regex.is_some(),
                "errorPresent": parsed.error.is_some(),
            })
        })
        .collect();
    assert_eq!(json!(actual), expected["output"], "parse mismatch");

    let expected = oracle.request(&json!({
        "id": 3,
        "operation": "match",
        "sessions": request_sessions,
        "query": "chair",
    }));
    let parsed = parse_search_query("chair");
    let actual: Vec<Value> = rust_sessions
        .iter()
        .map(|session| {
            let result = match_session(session, &parsed);
            json!({"matches": result.matches, "literal": result.literal, "score": result.score})
        })
        .collect();
    let expected = expected["output"].as_array().expect("match output");
    assert_eq!(actual.len(), expected.len(), "match count");
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(
            actual["matches"], expected["matches"],
            "match flag: {actual} vs {expected}"
        );
        assert_eq!(
            actual["literal"], expected["literal"],
            "literal flag: {actual} vs {expected}"
        );
        assert_eq!(
            actual["score"].as_f64(),
            expected["score"].as_f64(),
            "score: {actual} vs {expected}"
        );
    }
}

fn corpus() -> (Value, Vec<SessionInfo>) {
    let entries = [
        ("newer", 1_767_312_000_000, "brave"),
        ("older", 1_767_139_200_000, "brave"),
        ("candidate-a", 1_767_398_400_000, "could help arrange indexing results"),
        ("candidate-b", 1_767_312_000_000, "my computer chair broke"),
        ("candidate-c", 1_767_225_600_000, "could help arrange internal records"),
        ("candidate-d", 1_767_139_200_000, "replacement chair ordered"),
        ("separated-tokens", 1_767_312_000_000, "foo elsewhere bar"),
        ("complete-query", 1_767_139_200_000, "xxxxxxxxxxxxxxxx foo bar"),
        ("phrase", 1_767_225_600_000, "node\n\n   cve was discussed"),
        ("regex", 1_767_225_600_000, "Brave is great"),
        ("mixed", 1_767_225_600_000, "alpha chair"),
    ];
    let mut request_sessions = Vec::new();
    let mut rust_sessions = Vec::new();
    for (id, modified, text) in entries {
        let (request, info) = session(id, modified, text);
        request_sessions.push(request);
        rust_sessions.push(info);
    }
    (json!(request_sessions), rust_sessions)
}
