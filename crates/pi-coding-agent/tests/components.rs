//! Behavioral tests for the ported coding-agent selector components and
//! `session-selector-search` (ported from the TypeScript component tests).

use pi_js_compat::js_json::JsString;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_coding_agent::core::session_manager::SessionInfo;
use pi_coding_agent::modes::interactive::components::countdown_timer::CountdownTimer;
use pi_coding_agent::modes::interactive::components::extension_selector::{
    ExtensionSelectorComponent, ExtensionSelectorOptions,
};
use pi_coding_agent::modes::interactive::components::keybinding_hints::{
    KeyTextFormatOptions, format_key_text, key_display_text, key_hint, raw_key_hint,
};
use pi_coding_agent::modes::interactive::components::session_selector_search::{
    NameFilter, SortMode, filter_and_sort_sessions, has_session_messages, has_session_name, match_session,
    parse_search_query,
};
use pi_coding_agent::modes::interactive::components::user_message_selector::UserMessageSelectorComponent;
use pi_coding_agent::modes::interactive::theme::{load_theme_from_path, set_theme_instance};
use pi_tui::keybindings::{KeybindingsManager, set_keybindings, tui_keybindings};
use pi_tui::tui::Component;

fn install_dark_theme() {
    let theme = load_theme_from_path(
        &JsString::from(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("theme/dark.json")
                .to_str()
                .expect("fixture path must be UTF-8"),
        ),
        Some(pi_coding_agent::modes::interactive::theme::ColorMode::Truecolor),
    )
    .expect("dark theme");
    set_theme_instance(Arc::new(theme));
}

fn make_session(id: &str, modified: i64, first_message: &str) -> SessionInfo {
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

fn ids(sessions: &[SessionInfo]) -> Vec<&JsString> {
    sessions.iter().map(|session| &session.id).collect()
}

#[test]
fn filters_by_quoted_phrase_with_whitespace_normalization() {
    let sessions = vec![
        make_session("a", 1, "node\n\n   cve was discussed"),
        make_session("b", 2, "node something else"),
    ];
    let result = filter_and_sort_sessions(
        &sessions,
        &JsString::from("\"node cve\""),
        SortMode::Recent,
        NameFilter::All,
    );
    assert_eq!(ids(&result), ["a"]);
}

#[test]
fn filters_by_regex_case_insensitively() {
    let sessions = vec![
        make_session("a", 2, "Brave is great"),
        make_session("b", 3, "bravery is not the same"),
    ];
    let result = filter_and_sort_sessions(
        &sessions,
        &JsString::from("re:\\bbrave\\b"),
        SortMode::Recent,
        NameFilter::All,
    );
    assert_eq!(ids(&result), ["a"]);
}

#[test]
fn recent_sort_preserves_input_order() {
    let sessions = vec![
        make_session("newer", 3, "brave"),
        make_session("older", 1, "brave"),
        make_session("nomatch", 4, "something else"),
    ];
    let result = filter_and_sort_sessions(
        &sessions,
        &JsString::from("\"brave\""),
        SortMode::Recent,
        NameFilter::All,
    );
    assert_eq!(ids(&result), ["newer", "older"]);
}

#[test]
fn ranks_literal_matches_before_fuzzy_matches() {
    let sessions = vec![
        make_session("candidate-a", 4, "could help arrange indexing results"),
        make_session("candidate-b", 3, "my computer chair broke"),
        make_session("candidate-c", 2, "could help arrange internal records"),
        make_session("candidate-d", 1, "replacement chair ordered"),
    ];
    let result = filter_and_sort_sessions(&sessions, &JsString::from("chair"), SortMode::Recent, NameFilter::All);
    assert_eq!(
        ids(&result),
        ["candidate-b", "candidate-d", "candidate-a", "candidate-c"]
    );
}

#[test]
fn ranks_complete_plain_query_before_separated_tokens() {
    let sessions = vec![
        make_session("separated-tokens", 3, "foo elsewhere bar"),
        make_session("complete-query", 1, "xxxxxxxxxxxxxxxx foo bar"),
    ];
    let recent = filter_and_sort_sessions(&sessions, &JsString::from("foo bar"), SortMode::Recent, NameFilter::All);
    assert_eq!(ids(&recent), ["complete-query", "separated-tokens"]);
    let relevance = filter_and_sort_sessions(
        &sessions,
        &JsString::from("foo bar"),
        SortMode::Relevance,
        NameFilter::All,
    );
    assert_eq!(ids(&relevance), ["complete-query", "separated-tokens"]);
}

#[test]
fn preserves_recent_order_for_mixed_phrase_and_fuzzy_queries() {
    let sessions = vec![
        make_session("candidate-a", 3, "alpha could help arrange indexing results"),
        make_session("candidate-b", 1, "alpha chair"),
    ];
    let result = filter_and_sort_sessions(
        &sessions,
        &JsString::from("\"alpha\" chair"),
        SortMode::Recent,
        NameFilter::All,
    );
    assert_eq!(ids(&result), ["candidate-a", "candidate-b"]);
}

#[test]
fn relevance_sorts_by_score_then_modified_desc() {
    let sessions = vec![
        make_session("late", 3, "xxxx brave"),
        make_session("early", 1, "brave xxxx"),
    ];
    let result = filter_and_sort_sessions(
        &sessions,
        &JsString::from("\"brave\""),
        SortMode::Relevance,
        NameFilter::All,
    );
    assert_eq!(ids(&result), ["early", "late"]);

    let tie_sessions = vec![make_session("newer", 3, "brave"), make_session("older", 1, "brave")];
    let tie = filter_and_sort_sessions(
        &tie_sessions,
        &JsString::from("\"brave\""),
        SortMode::Relevance,
        NameFilter::All,
    );
    assert_eq!(ids(&tie), ["newer", "older"]);
}

#[test]
fn invalid_regex_yields_no_sessions() {
    let sessions = vec![make_session("a", 1, "brave")];
    let result = filter_and_sort_sessions(&sessions, &JsString::from("re:("), SortMode::Recent, NameFilter::All);
    assert!(result.is_empty());
    let parsed = parse_search_query(&JsString::from("re:("));
    assert!(parsed.error.is_some());
}

#[test]
fn excludes_sessions_without_messages() {
    let mut empty = make_session("empty", 3, "");
    empty.name = Some(JsString::from("Important Empty Session"));
    empty.message_count = 0;
    let sessions = vec![empty, make_session("normal", 2, "real conversation")];
    let result = filter_and_sort_sessions(&sessions, &JsString::from(""), SortMode::Recent, NameFilter::All);
    assert_eq!(ids(&result), ["normal"]);
}

#[test]
fn name_filter_selects_named_sessions_only() {
    let mut named1 = make_session("named1", 3, "blueberry");
    named1.name = Some(JsString::from("My Project"));
    let mut named2 = make_session("named2", 2, "blueberry");
    named2.name = Some(JsString::from("Another Named"));
    let sessions = vec![
        named1,
        named2,
        make_session("other1", 4, "blueberry"),
        make_session("other2", 1, "blueberry"),
    ];

    let all = filter_and_sort_sessions(&sessions, &JsString::from(""), SortMode::Recent, NameFilter::All);
    assert_eq!(ids(&all), ["named1", "named2", "other1", "other2"]);

    let named = filter_and_sort_sessions(&sessions, &JsString::from(""), SortMode::Recent, NameFilter::Named);
    assert_eq!(ids(&named), ["named1", "named2"]);

    let filtered = filter_and_sort_sessions(
        &sessions,
        &JsString::from("blueberry"),
        SortMode::Recent,
        NameFilter::Named,
    );
    assert_eq!(ids(&filtered), ["named1", "named2"]);

    let mut whitespace = make_session("whitespace", 1, "test");
    whitespace.name = Some(JsString::from("   "));
    let mut empty_name = make_session("empty", 2, "test");
    empty_name.name = Some(JsString::default());
    let mut real = make_session("named", 3, "test");
    real.name = Some(JsString::from("Real Name"));
    let result = filter_and_sort_sessions(
        &[whitespace, empty_name, real],
        &JsString::from(""),
        SortMode::Recent,
        NameFilter::Named,
    );
    assert_eq!(ids(&result), ["named"]);
}

#[test]
fn parses_search_queries() {
    let empty = parse_search_query(&JsString::from("   "));
    assert!(empty.tokens.is_empty());
    assert!(empty.regex.is_none());

    let tokens = parse_search_query(&JsString::from("foo \"node cve\" bar"));
    assert_eq!(tokens.tokens.len(), 3);
    assert_eq!(tokens.tokens[0].value, "foo");
    assert_eq!(
        tokens.tokens[1].kind,
        pi_coding_agent::modes::interactive::components::session_selector_search::SearchTokenKind::Phrase
    );
    assert_eq!(tokens.tokens[1].value, "node cve");
    assert_eq!(tokens.tokens[2].value, "bar");

    let unclosed = parse_search_query(&JsString::from("\"node cve"));
    assert!(unclosed.tokens.iter().all(|token| token.kind
        == pi_coding_agent::modes::interactive::components::session_selector_search::SearchTokenKind::Fuzzy));
    assert_eq!(unclosed.tokens.len(), 2);

    let regex = parse_search_query(&JsString::from("re:brave"));
    assert!(regex.regex.is_some());
    assert!(regex.error.is_none());

    assert_eq!(
        parse_search_query(&JsString::from("re:")).error.as_ref(),
        Some(&JsString::from("Empty regex"))
    );
}

#[test]
fn reports_session_name_and_message_presence() {
    let session = make_session("a", 1, "text");
    assert!(!has_session_name(&session));
    assert!(has_session_messages(&session));
    let mut named = session.clone();
    named.name = Some(JsString::from("  "));
    assert!(!has_session_name(&named));
    named.name = Some(JsString::from("Name"));
    assert!(has_session_name(&named));
    let mut empty = session;
    empty.message_count = 0;
    assert!(!has_session_messages(&empty));
    let parsed = parse_search_query(&JsString::from("text"));
    assert!(match_session(&named, &parsed).matches);
}

#[test]
fn formats_key_hint_text() {
    assert_eq!(
        format_key_text("ctrl+a/alt+b", KeyTextFormatOptions::default()),
        "ctrl+a/alt+b"
    );
    assert_eq!(
        format_key_text("ctrl+a/alt+b", KeyTextFormatOptions { capitalize: true }),
        "Ctrl+A/Alt+B"
    );
    assert_eq!(key_display_text("tui.select.confirm"), "Enter");
    assert_eq!(format_key_text("", KeyTextFormatOptions::default()), "");

    install_dark_theme();
    let hint = key_hint("tui.select.confirm", "select");
    assert!(hint.contains("enter"));
    assert!(hint.contains(" select"));
    let raw = raw_key_hint("↑↓", "navigate");
    assert!(raw.contains("↑↓"));
    assert!(raw.contains(" navigate"));
}

#[test]
fn extension_selector_toggles_tools_on_expand_keybinding() {
    install_dark_theme();
    let mut definitions = tui_keybindings();
    definitions.push((
        JsString::from("app.tools.expand"),
        pi_tui::keybindings::KeybindingDefinition {
            default_keys: vec![JsString::from("ctrl+o")],
            description: None,
        },
    ));
    set_keybindings(KeybindingsManager::new(definitions.clone(), Vec::new()));

    let toggles = Rc::new(std::cell::Cell::new(0));
    let toggle_sink = Rc::clone(&toggles);
    let mut selector = ExtensionSelectorComponent::new(
        &JsString::from("Title"),
        vec![JsString::from("one"), JsString::from("two")],
        Rc::new(|_| {}),
        Rc::new(|| {}),
        ExtensionSelectorOptions {
            on_toggle_tools_expanded: Some(Rc::new(move || toggle_sink.set(toggle_sink.get() + 1))),
            ..ExtensionSelectorOptions::default()
        },
    );
    selector.handle_input(&JsString::from("\u{0f}"));
    assert_eq!(toggles.get(), 1);

    set_keybindings(KeybindingsManager::new(tui_keybindings(), Vec::new()));
}

#[test]
fn countdown_timer_ticks_and_expires() {
    let ticks = Rc::new(std::cell::RefCell::new(Vec::new()));
    let renders = Rc::new(std::cell::Cell::new(0));
    let expired = Rc::new(std::cell::Cell::new(0));

    let tick_sink = Rc::clone(&ticks);
    let render_sink = Rc::clone(&renders);
    let expire_sink = Rc::clone(&expired);
    let mut timer = CountdownTimer::new(
        2500.0,
        Some(Rc::new(move || render_sink.set(render_sink.get() + 1))),
        Rc::new(move |seconds| tick_sink.borrow_mut().push(seconds)),
        Rc::new(move || expire_sink.set(expire_sink.get() + 1)),
    );
    assert_eq!(*ticks.borrow(), [3.0]);
    let first_deadline = timer.next_deadline().expect("scheduled");

    assert!(!timer.poll(first_deadline));
    assert!(!timer.poll(first_deadline + Duration::from_millis(500)));
    assert!(!timer.poll(first_deadline + Duration::from_secs(1)));
    assert!(timer.poll(first_deadline + Duration::from_secs(2)));
    assert_eq!(*ticks.borrow(), [3.0, 2.0, 1.0, 0.0]);
    assert_eq!(renders.get(), 3);
    assert_eq!(expired.get(), 1);
    assert!(timer.is_disposed());
    assert_eq!(timer.next_deadline(), None);
}

#[test]
fn user_message_selector_auto_cancels_when_empty() {
    install_dark_theme();
    let cancelled = Rc::new(std::cell::Cell::new(0));
    let cancel_sink = Rc::clone(&cancelled);
    let mut selector = UserMessageSelectorComponent::new(
        Vec::new(),
        Rc::new(|_| {}),
        Rc::new(move || cancel_sink.set(cancel_sink.get() + 1)),
        None,
    );
    let deadline = selector.auto_cancel_deadline().expect("scheduled");
    assert!(!selector.poll_auto_cancel(deadline - Duration::from_millis(1)));
    assert!(selector.poll_auto_cancel(deadline));
    assert_eq!(cancelled.get(), 1);
    assert!(!selector.poll_auto_cancel(Instant::now()));
    let rendered = selector.render(40);
    assert!(rendered.iter().any(|line| line.contains("No user messages found")));
}
