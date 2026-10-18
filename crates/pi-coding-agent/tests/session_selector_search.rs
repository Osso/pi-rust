//! Behavioral tests for `modes/interactive/components/session-selector-search.ts`.
//! Ports every case of `test/session-selector-search.test.ts`.

use pi_coding_agent::core::session_manager::SessionInfo;
use pi_coding_agent::modes::interactive::components::session_selector_search::{
    NameFilter, SortMode, filter_and_sort_sessions,
};

fn make_session(id: &str, modified_ms: i64, all_messages_text: &str) -> SessionInfo {
    SessionInfo {
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
    }
}

fn ids(sessions: &[SessionInfo]) -> Vec<&str> {
    sessions.iter().map(|session| session.id.as_str()).collect()
}

fn day(day: i64) -> i64 {
    // 2026-01-01T00:00:00.000Z + day offset.
    1_767_225_600_000 + day * 86_400_000
}

#[test]
fn filters_by_quoted_phrase_with_whitespace_normalization() {
    let sessions = vec![
        make_session("a", day(0), "node\n\n   cve was discussed"),
        make_session("b", day(1), "node something else"),
    ];
    let result = filter_and_sort_sessions(&sessions, "\"node cve\"", SortMode::Recent, NameFilter::All);
    assert_eq!(ids(&result), vec!["a"]);
}

#[test]
fn filters_by_regex_and_is_case_insensitive() {
    let sessions = vec![
        make_session("a", day(1), "Brave is great"),
        make_session("b", day(2), "bravery is not the same"),
    ];
    let result = filter_and_sort_sessions(&sessions, "re:\\bbrave\\b", SortMode::Recent, NameFilter::All);
    assert_eq!(ids(&result), vec!["a"]);
}

#[test]
fn recent_sort_preserves_input_order() {
    let sessions = vec![
        make_session("newer", day(2), "brave"),
        make_session("older", day(0), "brave"),
        make_session("nomatch", day(3), "something else"),
    ];
    let result = filter_and_sort_sessions(&sessions, "\"brave\"", SortMode::Recent, NameFilter::All);
    assert_eq!(ids(&result), vec!["newer", "older"]);
}

#[test]
fn ranks_literal_matches_first_while_preserving_recent_order_within_each_group() {
    let sessions = vec![
        make_session("candidate-a", day(3), "could help arrange indexing results"),
        make_session("candidate-b", day(2), "my computer chair broke"),
        make_session("candidate-c", day(1), "could help arrange internal records"),
        make_session("candidate-d", day(0), "replacement chair ordered"),
    ];
    let result = filter_and_sort_sessions(&sessions, "chair", SortMode::Recent, NameFilter::All);
    assert_eq!(
        ids(&result),
        vec!["candidate-b", "candidate-d", "candidate-a", "candidate-c"]
    );
}

#[test]
fn ranks_the_complete_plain_query_before_separated_literal_tokens() {
    let sessions = vec![
        make_session("separated-tokens", day(2), "foo elsewhere bar"),
        make_session("complete-query", day(0), "xxxxxxxxxxxxxxxx foo bar"),
    ];
    let recent = filter_and_sort_sessions(&sessions, "foo bar", SortMode::Recent, NameFilter::All);
    assert_eq!(ids(&recent), vec!["complete-query", "separated-tokens"]);
    let relevance = filter_and_sort_sessions(&sessions, "foo bar", SortMode::Relevance, NameFilter::All);
    assert_eq!(ids(&relevance), vec!["complete-query", "separated-tokens"]);
}

#[test]
fn preserves_recent_ordering_for_mixed_phrase_and_fuzzy_token_queries() {
    let sessions = vec![
        make_session("candidate-a", day(2), "alpha could help arrange indexing results"),
        make_session("candidate-b", day(0), "alpha chair"),
    ];
    let result = filter_and_sort_sessions(&sessions, "\"alpha\" chair", SortMode::Recent, NameFilter::All);
    assert_eq!(ids(&result), vec!["candidate-a", "candidate-b"]);
}

#[test]
fn relevance_sort_orders_by_score_and_tie_breaks_by_modified_desc() {
    let sessions = vec![
        make_session("late", day(2), "xxxx brave"),
        make_session("early", day(0), "brave xxxx"),
    ];
    let result = filter_and_sort_sessions(&sessions, "\"brave\"", SortMode::Relevance, NameFilter::All);
    assert_eq!(ids(&result), vec!["early", "late"]);

    let tie_sessions = vec![
        make_session("newer", day(2), "brave"),
        make_session("older", day(0), "brave"),
    ];
    let result = filter_and_sort_sessions(&tie_sessions, "\"brave\"", SortMode::Relevance, NameFilter::All);
    assert_eq!(ids(&result), vec!["newer", "older"]);
}

#[test]
fn returns_empty_list_for_invalid_regex() {
    let sessions = vec![make_session("a", day(0), "brave")];
    let result = filter_and_sort_sessions(&sessions, "re:(", SortMode::Recent, NameFilter::All);
    assert!(result.is_empty());
}

#[test]
fn excludes_sessions_with_no_messages() {
    let mut empty = make_session("empty", day(2), "");
    empty.name = Some("Important Empty Session".to_string());
    empty.message_count = 0;
    let sessions = vec![empty, make_session("normal", day(1), "real conversation")];
    let result = filter_and_sort_sessions(&sessions, "", SortMode::Recent, NameFilter::All);
    assert_eq!(ids(&result), vec!["normal"]);
}

fn named_sessions() -> Vec<SessionInfo> {
    let mut named1 = make_session("named1", day(2), "blueberry");
    named1.name = Some("My Project".to_string());
    let mut named2 = make_session("named2", day(1), "blueberry");
    named2.name = Some("Another Named".to_string());
    vec![
        named1,
        named2,
        make_session("other1", day(3), "blueberry"),
        make_session("other2", day(0), "blueberry"),
    ]
}

#[test]
fn name_filter_all_returns_all_sessions() {
    let result = filter_and_sort_sessions(&named_sessions(), "", SortMode::Recent, NameFilter::All);
    assert_eq!(ids(&result), vec!["named1", "named2", "other1", "other2"]);
}

#[test]
fn name_filter_named_returns_only_named_sessions() {
    let result = filter_and_sort_sessions(&named_sessions(), "", SortMode::Recent, NameFilter::Named);
    assert_eq!(ids(&result), vec!["named1", "named2"]);
}

#[test]
fn name_filter_applies_before_search_query() {
    let result = filter_and_sort_sessions(&named_sessions(), "blueberry", SortMode::Recent, NameFilter::Named);
    assert_eq!(ids(&result), vec!["named1", "named2"]);
}

#[test]
fn name_filter_excludes_whitespace_only_names() {
    let mut whitespace = make_session("whitespace", day(0), "test");
    whitespace.name = Some("   ".to_string());
    let mut empty = make_session("empty", day(1), "test");
    empty.name = Some(String::new());
    let mut named = make_session("named", day(2), "test");
    named.name = Some("Real Name".to_string());
    let sessions = vec![whitespace, empty, named];
    let result = filter_and_sort_sessions(&sessions, "", SortMode::Recent, NameFilter::Named);
    assert_eq!(ids(&result), vec!["named"]);
}
