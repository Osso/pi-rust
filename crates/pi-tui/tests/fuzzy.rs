use pi_tui::fuzzy::{FuzzyMatch, fuzzy_filter, fuzzy_match};

#[test]
fn empty_query_matches_everything_with_score_zero() {
    assert_eq!(
        fuzzy_match("", "anything"),
        FuzzyMatch {
            matches: true,
            score: 0.0
        }
    );
}

#[test]
fn query_longer_than_text_does_not_match() {
    assert!(!fuzzy_match("longquery", "short").matches);
}

#[test]
fn exact_match_scores_negative() {
    let result = fuzzy_match("test", "test");
    assert!(result.matches);
    assert_eq!(result.score, -159.4);
}

#[test]
fn characters_must_appear_in_order() {
    assert!(fuzzy_match("abc", "aXbXc").matches);
    assert!(!fuzzy_match("abc", "cba").matches);
}

#[test]
fn matching_is_case_insensitive() {
    assert!(fuzzy_match("ABC", "abc").matches);
    assert!(fuzzy_match("abc", "ABC").matches);
    assert_eq!(fuzzy_match("ABC", "abc").score, -139.7);
}

#[test]
fn consecutive_matches_score_better_than_scattered_matches() {
    let consecutive = fuzzy_match("foo", "foobar");
    let scattered = fuzzy_match("foo", "f_o_o_bar");
    assert!(consecutive.matches && scattered.matches);
    assert!(consecutive.score < scattered.score);
    assert_eq!(consecutive.score, -39.699999999999996);
    assert_eq!(scattered.score, -30.400000000000002);
}

#[test]
fn word_boundary_matches_score_better() {
    let at_boundary = fuzzy_match("fb", "foo-bar");
    let not_at_boundary = fuzzy_match("fb", "afbx");
    assert!(at_boundary.matches && not_at_boundary.matches);
    assert!(at_boundary.score < not_at_boundary.score);
    assert_eq!(at_boundary.score, -18.6);
    assert_eq!(not_at_boundary.score, -4.7);
}

#[test]
fn swapped_alphanumeric_tokens_match() {
    let result = fuzzy_match("codex52", "gpt-5.2-codex");
    assert!(result.matches);
    assert_eq!(result.score, -64.99999999999999);
}

#[test]
fn astral_characters_are_scored_in_utf16_units() {
    let result = fuzzy_match("😀", "a😀b");
    assert!(result.matches);
    assert_eq!(result.score, -4.7);
}

#[test]
fn filtering_returns_items_unchanged_for_empty_query() {
    let items = vec!["apple", "banana", "cherry"];
    let result = fuzzy_filter(&items, "", |item| vec![item.to_string()]);
    assert_eq!(result, items);
}

#[test]
fn filtering_drops_non_matching_items() {
    let items = vec!["apple", "banana", "cherry"];
    let result = fuzzy_filter(&items, "an", |item| vec![item.to_string()]);
    assert_eq!(result, vec!["banana"]);
}

#[test]
fn filtering_sorts_by_match_quality() {
    let items = vec!["a_p_p", "app", "application"];
    let result = fuzzy_filter(&items, "app", |item| vec![item.to_string()]);
    assert_eq!(result, vec!["app", "application", "a_p_p"]);
}

#[test]
fn filtering_prioritizes_exact_matches() {
    let items = vec!["clone", "cl"];
    let result = fuzzy_filter(&items, "cl", |item| vec![item.to_string()]);
    assert_eq!(result, vec!["cl", "clone"]);
}

#[test]
fn filtering_supports_multiple_candidates_without_spanning_them() {
    let items = vec![("gpt-5.4", "openai-codex")];
    let get_candidates = |item: &(&str, &str)| {
        vec![
            item.0.to_string(),
            format!("{}/{}", item.1, item.0),
            format!("{} {}", item.1, item.0),
        ]
    };
    assert_eq!(fuzzy_filter(&items, "gpt-5.4", get_candidates), items);
    assert_eq!(fuzzy_filter(&items, "openai/gpt", get_candidates), items);
    assert!(fuzzy_filter(&items, "gpt-5.5", get_candidates).is_empty());
}

#[test]
fn filtering_splits_whitespace_and_slash_separated_tokens() {
    let items = vec![("gpt-5.5", "openai-codex")];
    let result = fuzzy_filter(&items, "openai-codex/gpt-5.5", |item| {
        vec![format!("{} {}", item.0, item.1)]
    });
    assert_eq!(result, items);
}
