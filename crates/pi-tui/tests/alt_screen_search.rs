//! Ported from `packages/tui/test/tui-alt-screen.test.ts` search cases.

use std::cell::RefCell;
use std::rc::Rc;

use pi_tui::alt_screen_search::{
    AltScreenSearchComponent, AltScreenSearchMatch, AltScreenSearchSegment, find_alt_screen_search_matches,
    get_alt_screen_search_match_key,
};
use pi_tui::tui::{Component, Focusable};

fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn segment(row: i64, start_col: i64, end_col: i64) -> AltScreenSearchSegment {
    AltScreenSearchSegment {
        row,
        start_col,
        end_col,
    }
}

fn input_line(width: usize, text: &str) -> String {
    let available = width - 2;
    let cursor = "\x1b[7m \x1b[27m";
    format!("> {text}{cursor}{}", " ".repeat(available - text.len() - 1))
}

#[test]
fn searches_normalized_rendered_transcript_text_across_rows() {
    assert_eq!(
        find_alt_screen_search_matches(&lines(&["alpha QUICK", "brown fox"]), "quick brown"),
        vec![AltScreenSearchMatch {
            segments: vec![segment(0, 6, 11), segment(1, 0, 5)],
        }]
    );
}

#[test]
fn empty_or_whitespace_queries_return_no_matches() {
    assert!(find_alt_screen_search_matches(&lines(&["alpha"]), "").is_empty());
    assert!(find_alt_screen_search_matches(&lines(&["alpha"]), "   \t ").is_empty());
}

#[test]
fn collapses_whitespace_in_queries_and_matches_case_insensitively() {
    let matches = find_alt_screen_search_matches(&lines(&["NEEDLE   one"]), " needle one ");
    assert_eq!(
        matches,
        vec![AltScreenSearchMatch {
            segments: vec![segment(0, 0, 6), segment(0, 9, 12)],
        }]
    );
}

#[test]
fn treats_line_boundaries_and_leading_whitespace_as_single_spaces() {
    let matches = find_alt_screen_search_matches(&lines(&["  alpha", "  beta"]), "alpha beta");
    assert_eq!(
        matches,
        vec![AltScreenSearchMatch {
            segments: vec![segment(0, 2, 7), segment(1, 2, 6)],
        }]
    );
}

#[test]
fn reports_every_match_in_order() {
    let matches = find_alt_screen_search_matches(&lines(&["one two one", "one"]), "one");
    assert_eq!(
        matches,
        vec![
            AltScreenSearchMatch {
                segments: vec![segment(0, 0, 3)],
            },
            AltScreenSearchMatch {
                segments: vec![segment(0, 8, 11)],
            },
            AltScreenSearchMatch {
                segments: vec![segment(1, 0, 3)],
            },
        ]
    );
}

#[test]
fn strips_terminal_sequences_before_mapping_columns() {
    let matches = find_alt_screen_search_matches(&lines(&["\x1b[1malpha\x1b[0m beta"]), "alpha beta");
    assert_eq!(
        matches,
        vec![AltScreenSearchMatch {
            segments: vec![segment(0, 0, 5), segment(0, 6, 10)],
        }]
    );
}

#[test]
fn maps_wide_graphemes_to_cell_columns() {
    let matches = find_alt_screen_search_matches(&lines(&["A界🙂e\u{301}Z"]), "界🙂");
    assert_eq!(
        matches,
        vec![AltScreenSearchMatch {
            segments: vec![segment(0, 1, 5)],
        }]
    );
}

#[test]
fn builds_stable_match_keys() {
    let matches = find_alt_screen_search_matches(&lines(&["needle"]), "needle");
    assert_eq!(get_alt_screen_search_match_key(&matches[0]), "0:0:0:6");
    assert_eq!(
        get_alt_screen_search_match_key(&AltScreenSearchMatch { segments: vec![] }),
        ""
    );
}

#[test]
fn renders_the_title_status_and_input_line() {
    let queries: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&queries);
    let mut component = AltScreenSearchComponent::new(Rc::new(move |query: &str| {
        sink.borrow_mut().push(query.to_string());
    }));

    let initial = component.render(40);
    assert_eq!(initial.len(), 2);
    assert_eq!(initial[0], format!("\x1b[7m Find transcript{}\x1b[27m", " ".repeat(24)));
    assert_eq!(initial[1], input_line(40, ""));

    component.set_result(-1, 0);
    for character in ["n", "e", "e"] {
        component.handle_input(character);
    }
    let rendered = component.render(40);
    assert_eq!(
        rendered[0],
        format!("\x1b[7m Find transcript{}No matches \x1b[27m", " ".repeat(13))
    );
    assert_eq!(
        *queries.borrow(),
        vec!["n".to_string(), "ne".to_string(), "nee".to_string()]
    );

    component.set_result(1, 3);
    let rendered = component.render(40);
    assert_eq!(
        rendered[0],
        format!("\x1b[7m Find transcript{}2/3 \x1b[27m", " ".repeat(20))
    );
}

#[test]
fn truncates_the_title_to_the_safe_width() {
    let mut component = AltScreenSearchComponent::new(Rc::new(|_: &str| {}));
    let rendered = component.render(8);
    assert_eq!(rendered[0], "\x1b[7m Find tr\x1b[0m\x1b[27m");
    assert_eq!(rendered.len(), 2);
    assert_eq!(rendered[1], input_line(8, ""));
}

#[test]
fn forwards_focus_to_the_input_and_only_reports_changed_queries() {
    let queries: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&queries);
    let mut component = AltScreenSearchComponent::new(Rc::new(move |query: &str| {
        sink.borrow_mut().push(query.to_string());
    }));

    component.set_focused(true);
    assert!(component.focused());
    let rendered = component.render(20);
    assert!(rendered[1].contains("\x1b_pi:c\x07"));

    component.handle_input("a");
    component.handle_input("b");
    component.handle_input("\x1b[D");
    component.handle_input("\x7f");
    assert_eq!(
        *queries.borrow(),
        vec!["a".to_string(), "ab".to_string(), "b".to_string()]
    );
}
