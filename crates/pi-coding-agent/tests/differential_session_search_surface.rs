use crate::shared_oracle as oracle;
use pi_coding_agent::{
    core::session_manager::SessionInfo,
    modes::interactive::components::session_selector_search::{
        NameFilter, SearchMode, SearchTokenKind, SortMode, filter_and_sort_sessions, has_session_messages,
        has_session_name, match_session, parse_search_query,
    },
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

#[test]
fn full_search_surface_matches_frozen_with_raw_utf16() {
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    let raw = JsString::from_utf16(vec![0xd800, 0x78, 0xdc00]);
    let mut sessions: Vec<_> = [
        ("one", "my computer chair broke", 4),
        ("two", "could help arrange indexing", 5),
        ("three", "replacement chair ordered", 3),
        ("empty", "chair", 6),
    ]
    .into_iter()
    .map(|(id, text, modified)| SessionInfo {
        path: format!("/tmp/{id}").into(),
        id: id.into(),
        cwd: "/work".into(),
        name: None,
        is_archived: None,
        parent_session_path: None,
        created: 0.0,
        modified: modified as f64,
        message_count: usize::from(id != "empty"),
        first_message: text.into(),
    })
    .collect();
    sessions[0].name = Some(raw.clone());
    sessions[1].name = Some("\u{feff} \t".into());
    sessions[2].first_message = JsString::from("😀 chair ") + &raw;
    let mut queries: Vec<JsString> = [
        "",
        "\u{feff} \t",
        "chair",
        "\"chair\"",
        "\"chair\" replacement",
        "\"unclosed",
        "re:",
        "re:(",
        "re:(?i--m:a)",
        "re:ch(?=air)",
        "re:(a)\\1",
        "re:\\bchair\\b",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    queries.extend([
        raw.clone(),
        JsString::from("\"") + &raw + "\"",
        JsString::from("re:") + &raw,
    ]);
    for query in queries {
        let request = json!({"id":1,"module":"modes/interactive/components/session-selector-search.ts","operation":"sessionSearch","queryUnits":query.units(),"sessions":sessions.iter().map(|s|json!({"id":s.id,"cwd":s.cwd,"nameUnits":s.name.as_ref().map(JsString::units),"firstMessageUnits":s.first_message.units(),"modified":s.modified,"messageCount":s.message_count})).collect::<Vec<_>>()});
        let response = oracle.request(&request);
        assert_eq!(response["ok"], true, "{response}");
        let mut expected = response["output"].clone();
        let parsed = parse_search_query(&query);
        let tokens: Vec<_> = parsed
            .tokens
            .iter()
            .map(|token| {
                json!([
                    match token.kind {
                        SearchTokenKind::Phrase => "phrase",
                        SearchTokenKind::Fuzzy => "fuzzy",
                    },
                    token.value.units()
                ])
            })
            .collect();
        let matches:Vec<_>=sessions.iter().map(|session|{let result=match_session(session,&parsed);json!({"matches":result.matches,"literal":result.literal,"score":pi_js_compat::number::js_number_value(result.score)})}).collect();
        let sorted: Vec<Vec<JsString>> = [SortMode::Recent, SortMode::Relevance, SortMode::Threaded]
            .into_iter()
            .flat_map(|sort| {
                [NameFilter::All, NameFilter::Named].into_iter().map({
                    let sessions = &sessions;
                    let query = &query;
                    move |filter| {
                        filter_and_sort_sessions(sessions, query, sort, filter)
                            .into_iter()
                            .map(|s| s.id)
                            .collect()
                    }
                })
            })
            .collect();
        let mut actual = json!({"mode":match parsed.mode{SearchMode::Tokens=>"tokens",SearchMode::Regex=>"regex"},"tokens":tokens,"regex":parsed.regex.is_some(),"errorUnits":parsed.error.as_ref().map(JsString::units),"matches":matches,"names":sessions.iter().map(has_session_name).collect::<Vec<_>>(),"messages":sessions.iter().map(has_session_messages).collect::<Vec<_>>(),"sorted":sorted});
        if let Some(error) = &parsed.error {
            assert!(!expected["errorUnits"].is_null());
            if error != "Empty regex" {
                let pattern = query.trim().strip_prefix("re:").unwrap().trim();
                let prefix = JsString::from("Invalid regular expression: /") + pattern + "/i: ";
                let node_units: Vec<u16> = serde_json::from_value(expected["errorUnits"].clone()).unwrap();
                assert!(error.starts_with(&prefix));
                assert!(node_units.starts_with(prefix.units()));
                // AGENTS.md approved 2026-10-22: only regex reason wording may differ.
                actual["errorUnits"] = Value::Null;
                expected["errorUnits"] = Value::Null;
            }
        }
        assert_eq!(actual, expected, "query={query:?}");
    }
}
