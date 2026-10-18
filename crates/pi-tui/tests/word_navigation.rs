use pi_tui::word_navigation::{WordNavigationOptions, WordSegment, find_word_backward, find_word_forward};

#[test]
fn backward_basic_words() {
    let text = "hello world";
    assert_eq!(find_word_backward(text, 11, None), 6);
    assert_eq!(find_word_backward(text, 6, None), 0);
}

#[test]
fn backward_dotted_path_and_colon() {
    assert_eq!(find_word_backward("foo.bar", 7, None), 4);
    assert_eq!(find_word_backward("foo.bar", 4, None), 3);
    assert_eq!(find_word_backward("foo.bar", 3, None), 0);

    assert_eq!(find_word_backward("foo:bar", 7, None), 4);
    assert_eq!(find_word_backward("foo:bar", 4, None), 3);
    assert_eq!(find_word_backward("foo:bar", 3, None), 0);
}

#[test]
fn backward_slash_separated_path() {
    let text = "path/to/file";
    assert_eq!(find_word_backward(text, 12, None), 8);
    assert_eq!(find_word_backward(text, 8, None), 7);
    assert_eq!(find_word_backward(text, 7, None), 5);
    assert_eq!(find_word_backward(text, 5, None), 4);
    assert_eq!(find_word_backward(text, 4, None), 0);
}

#[test]
fn backward_cjk_uses_dictionary_segments() {
    let text = "你好世界 test";
    assert_eq!(text.len(), 17);
    assert_eq!(find_word_backward(text, text.len(), None), 13);
    assert_eq!(find_word_backward(text, 13, None), 6);
    assert_eq!(find_word_backward(text, 6, None), 0);
}

#[test]
fn backward_skips_whitespace_at_boundaries() {
    let text = "  hello  ";
    assert_eq!(find_word_backward(text, 9, None), 2);
    assert_eq!(find_word_backward(text, 2, None), 0);
}

#[test]
fn backward_skips_punctuation_run() {
    let text = "foo...bar";
    assert_eq!(find_word_backward(text, 9, None), 6);
    assert_eq!(find_word_backward(text, 6, None), 3);
    assert_eq!(find_word_backward(text, 3, None), 0);
}

#[test]
fn backward_cursor_at_zero_returns_zero() {
    assert_eq!(find_word_backward("hello", 0, None), 0);
}

#[test]
fn forward_basic_words() {
    let text = "hello world";
    assert_eq!(find_word_forward(text, 0, None), 5);
    assert_eq!(find_word_forward(text, 5, None), 11);
}

#[test]
fn forward_dotted_and_colon() {
    assert_eq!(find_word_forward("foo.bar", 0, None), 3);
    assert_eq!(find_word_forward("foo.bar", 3, None), 4);
    assert_eq!(find_word_forward("foo.bar", 4, None), 7);

    assert_eq!(find_word_forward("foo:bar", 0, None), 3);
    assert_eq!(find_word_forward("foo:bar", 3, None), 4);
    assert_eq!(find_word_forward("foo:bar", 4, None), 7);
}

#[test]
fn forward_slash_separated_path() {
    let text = "path/to/file";
    assert_eq!(find_word_forward(text, 0, None), 4);
    assert_eq!(find_word_forward(text, 4, None), 5);
    assert_eq!(find_word_forward(text, 5, None), 7);
    assert_eq!(find_word_forward(text, 7, None), 8);
    assert_eq!(find_word_forward(text, 8, None), 12);
}

#[test]
fn forward_cjk_walks_to_end() {
    let text = "你好世界 test";
    let first_end = find_word_forward(text, 0, None);
    assert!(first_end > 0);
    assert!(first_end <= 6);

    let mut position = 0;
    while position < text.len() {
        let next = find_word_forward(text, position, None);
        if next == position {
            break;
        }
        position = next;
    }
    assert_eq!(position, text.len());
}

#[test]
fn forward_skips_whitespace_at_boundaries() {
    let text = "  hello  ";
    assert_eq!(find_word_forward(text, 0, None), 7);
    assert_eq!(find_word_forward(text, 7, None), 9);
}

#[test]
fn forward_skips_punctuation_run() {
    let text = "foo...bar";
    assert_eq!(find_word_forward(text, 0, None), 3);
    assert_eq!(find_word_forward(text, 3, None), 6);
    assert_eq!(find_word_forward(text, 6, None), 9);
}

#[test]
fn forward_cursor_at_end_returns_end() {
    assert_eq!(find_word_forward("hello", 5, None), 5);
}

const MARKER: &str = "[paste #1 +5 lines]";

fn atomic(segment: &str) -> bool {
    segment == MARKER
}

fn segment_map(input: &str) -> Vec<WordSegment> {
    let text = format!("hello {MARKER} world");
    let all = [
        ("hello", 0usize, true),
        (" ", 5, false),
        (MARKER, 6, true),
        (" ", 25, false),
        ("world", 26, true),
    ];
    let prefix = &text[..26];
    let forward = &text[6..];
    let selected: &[(usize, usize)] = if input == text {
        &[(0, 5)]
    } else if input == prefix {
        &[(0, 4)]
    } else if input == forward {
        &[(2, 5)]
    } else {
        &[]
    };
    selected
        .iter()
        .flat_map(|(start, end)| all[*start..*end].iter())
        .map(|(segment, _, is_word_like)| WordSegment {
            segment: (*segment).to_string(),
            is_word_like: *is_word_like,
        })
        .collect()
}

#[test]
fn atomic_segments_are_skipped_as_one_unit() {
    let text = format!("hello {MARKER} world");
    let options = WordNavigationOptions {
        segment: Some(&segment_map),
        is_atomic_segment: Some(&atomic),
    };

    assert_eq!(find_word_backward(&text, text.len(), Some(&options)), 26);
    assert_eq!(find_word_backward(&text, 26, Some(&options)), 6);
    assert_eq!(find_word_forward(&text, 6, Some(&options)), 6 + MARKER.len());
}

#[test]
fn cursor_past_end_is_clamped_to_end() {
    assert_eq!(find_word_backward("hello", 99, None), 0);
    assert_eq!(find_word_forward("hello", 99, None), 5);
}

#[test]
fn cursor_inside_multibyte_character_snaps_back_to_boundary() {
    assert_eq!(find_word_backward("你好世界", 1, None), 0);
    assert_eq!(find_word_forward("你好世界", 1, None), 6);
}
