//! Ported from `packages/tui/test/editor.test.ts` (`Word wrapping`).

use pi_tui::components::editor::{TextSegment, word_wrap_line};
use pi_tui::utils::visible_width;

fn chunks(line: &str, width: usize) -> Vec<String> {
    word_wrap_line(line, width, None)
        .into_iter()
        .map(|chunk| chunk.text)
        .collect()
}

fn segments(_line: &str, pieces: &[&str]) -> Vec<TextSegment> {
    let mut index = 0;
    pieces
        .iter()
        .map(|piece| {
            let segment = TextSegment {
                text: (*piece).to_string(),
                index,
            };
            index += piece.len();
            segment
        })
        .collect()
}

fn reconstruct(line: &str, width: usize, pieces: Option<&[TextSegment]>) -> String {
    word_wrap_line(line, width, pieces)
        .into_iter()
        .map(|chunk| line[chunk.start_index..chunk.end_index].to_string())
        .collect()
}

#[test]
fn wraps_at_word_boundaries_instead_of_mid_word() {
    let line = "Hello world this is a test of word wrapping functionality";
    let rendered = chunks(line, 40);
    assert!(rendered.len() > 1);
    for chunk in rendered {
        assert!(visible_width(&chunk) <= 40);
    }
}

#[test]
fn does_not_start_lines_with_leading_whitespace_after_word_wrap() {
    let rendered = chunks("Word1 Word2 Word3 Word4 Word5 Word6", 20);
    for chunk in rendered {
        assert!(!chunk.starts_with(' '), "chunk starts with space: {chunk:?}");
    }
}

#[test]
fn breaks_long_words_at_character_level() {
    let line = "Check https://example.com/very/long/path/that/exceeds/width here";
    for chunk in word_wrap_line(line, 30, None) {
        assert!(visible_width(&chunk.text) <= 30);
    }
}

#[test]
fn preserves_multiple_spaces_within_words_on_same_line() {
    let rendered = chunks("Word1   Word2    Word3", 50);
    assert!(rendered[0].contains("Word1   Word2"));
}

#[test]
fn handles_empty_string() {
    let rendered = chunks("", 40);
    assert_eq!(rendered, vec![String::new()]);
}

#[test]
fn handles_single_word_that_fits_exactly() {
    let rendered = word_wrap_line("1234567890", 11, None);
    assert_eq!(rendered.len(), 1);
    assert_eq!(rendered[0].text, "1234567890");
}

#[test]
fn wraps_word_to_next_line_when_it_ends_exactly_at_terminal_width() {
    let rendered = chunks("hello world test", 11);
    assert_eq!(rendered, vec!["hello ", "world test"]);
}

#[test]
fn keeps_whitespace_at_terminal_width_boundary_on_same_line() {
    let rendered = chunks("hello world test", 12);
    assert_eq!(rendered, vec!["hello world ", "test"]);
}

#[test]
fn handles_unbreakable_word_filling_width_exactly_followed_by_space() {
    let rendered = chunks("aaaaaaaaaaaa aaaa", 12);
    assert_eq!(rendered, vec!["aaaaaaaaaaaa", " aaaa"]);
}

#[test]
fn wraps_word_to_next_line_when_it_fits_width_but_not_remaining_space() {
    let rendered = chunks("      aaaaaaaaaaaa", 12);
    assert_eq!(rendered, vec!["      ", "aaaaaaaaaaaa"]);
}

#[test]
fn keeps_word_with_multi_space_and_following_word_together_when_they_fit() {
    let rendered = chunks("Lorem ipsum dolor sit amet,    consectetur", 30);
    assert_eq!(rendered, vec!["Lorem ipsum dolor sit ", "amet,    consectetur"]);
}

#[test]
fn keeps_word_with_multi_space_and_following_word_when_they_fill_width_exactly() {
    let rendered = chunks("Lorem ipsum dolor sit amet,              consectetur", 30);
    assert_eq!(
        rendered,
        vec!["Lorem ipsum dolor sit ", "amet,              consectetur"]
    );
}

#[test]
fn splits_when_word_plus_multi_space_plus_word_exceeds_width() {
    let rendered = chunks("Lorem ipsum dolor sit amet,               consectetur", 30);
    assert_eq!(
        rendered,
        vec!["Lorem ipsum dolor sit ", "amet,               ", "consectetur"]
    );
}

#[test]
fn breaks_long_whitespace_at_line_boundary() {
    let rendered = chunks("Lorem ipsum dolor sit amet,                         consectetur", 30);
    assert_eq!(
        rendered,
        vec![
            "Lorem ipsum dolor sit ",
            "amet,                         ",
            "consectetur"
        ]
    );
}

#[test]
fn breaks_long_whitespace_at_line_boundary_2() {
    let rendered = chunks("Lorem ipsum dolor sit amet,                          consectetur", 30);
    assert_eq!(
        rendered,
        vec![
            "Lorem ipsum dolor sit ",
            "amet,                         ",
            " consectetur"
        ]
    );
}

#[test]
fn breaks_whitespace_spanning_full_lines() {
    let rendered = chunks(
        "Lorem ipsum dolor sit amet,                                     consectetur",
        30,
    );
    assert_eq!(
        rendered,
        vec![
            "Lorem ipsum dolor sit ",
            "amet,                         ",
            "            consectetur"
        ]
    );
}

#[test]
fn force_breaks_when_wide_char_after_word_boundary_wrap_still_overflows() {
    let line = format!(" {}你", "a".repeat(186));
    let rendered = word_wrap_line(&line, 187, None);
    for chunk in &rendered {
        assert!(visible_width(&chunk.text) <= 187);
    }
    assert_eq!(reconstruct(&line, 187, None), line);
}

#[test]
fn splits_oversized_atomic_segment_across_multiple_chunks() {
    let marker = "[paste #1 +20 lines]";
    let line = format!("A{marker}B");
    let pieces = segments(&line, &["A", marker, "B"]);
    let rendered = word_wrap_line(&line, 10, Some(&pieces));
    for chunk in &rendered {
        assert!(visible_width(&chunk.text) <= 10);
    }
    assert_eq!(reconstruct(&line, 10, Some(&pieces)), line);
}

#[test]
fn splits_oversized_atomic_segment_at_start_of_line() {
    let marker = "[paste #1 +20 lines]";
    let line = format!("{marker}B");
    let pieces = segments(&line, &[marker, "B"]);
    let rendered = word_wrap_line(&line, 10, Some(&pieces));
    for chunk in &rendered {
        assert!(visible_width(&chunk.text) <= 10);
    }
    assert!(rendered.last().unwrap().text.contains('B'));
    assert_eq!(reconstruct(&line, 10, Some(&pieces)), line);
}

#[test]
fn splits_oversized_atomic_segment_at_end_of_line() {
    let marker = "[paste #1 +20 lines]";
    let line = format!("A{marker}");
    let pieces = segments(&line, &["A", marker]);
    let rendered = word_wrap_line(&line, 10, Some(&pieces));
    for chunk in &rendered {
        assert!(visible_width(&chunk.text) <= 10);
    }
    assert_eq!(rendered[0].text, "A");
    assert_eq!(reconstruct(&line, 10, Some(&pieces)), line);
}

#[test]
fn splits_consecutive_oversized_atomic_segments() {
    let m1 = "[paste #1 +20 lines]";
    let m2 = "[paste #2 +30 lines]";
    let line = format!("{m1}{m2}");
    let pieces = segments(&line, &[m1, m2]);
    let rendered = word_wrap_line(&line, 10, Some(&pieces));
    for chunk in &rendered {
        assert!(visible_width(&chunk.text) <= 10);
    }
    assert_eq!(reconstruct(&line, 10, Some(&pieces)), line);
}

#[test]
fn wraps_normally_after_oversized_atomic_segment() {
    let marker = "[paste #1 +20 lines]";
    let line = format!("{marker} hello world");
    let pieces = segments(
        &line,
        &[marker, " ", "h", "e", "l", "l", "o", " ", "w", "o", "r", "l", "d"],
    );
    let rendered = word_wrap_line(&line, 10, Some(&pieces));
    for chunk in &rendered {
        assert!(visible_width(&chunk.text) <= 10);
    }
    assert_eq!(rendered.last().unwrap().text, "world");
    assert_eq!(reconstruct(&line, 10, Some(&pieces)), line);
}
