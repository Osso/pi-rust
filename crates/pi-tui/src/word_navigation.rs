//! Word navigation over ICU word segments.
//!
//! Cursor positions are UTF-8 byte offsets. The TypeScript source uses UTF-16
//! code-unit offsets; both functions operate on the same boundaries, and byte
//! offsets are the typed Rust mapping. Non-boundary cursors snap down to the
//! preceding character boundary instead of addressing half a code point.

use icu_segmenter::WordSegmenter;
use icu_segmenter::options::{WordBreakInvariantOptions, WordType};

use crate::unicode_tables::is_mark;
use crate::utils::{is_punctuation_char_value, is_whitespace_char};

/// One word segment, mirroring `Intl.SegmentData`.
///
/// `index` and `input` are omitted: the algorithms only consume the segment
/// text and `isWordLike`, and both are derivable from the surrounding slice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordSegment {
    pub segment: String,
    pub is_word_like: bool,
}

/// Custom segmenter callback.
pub type SegmentCallback<'a> = &'a dyn Fn(&str) -> Vec<WordSegment>;

/// Options for word navigation functions.
///
/// When omitted, ICU word segmentation (the engine behind
/// `Intl.Segmenter`) supplies the default segments.
#[derive(Default)]
pub struct WordNavigationOptions<'a> {
    /// Custom segmenter returning word segments for the given text.
    pub segment: Option<SegmentCallback<'a>>,
    /// Predicate identifying atomic segments treated as single units.
    pub is_atomic_segment: Option<&'a dyn Fn(&str) -> bool>,
}

/// Find the cursor position after moving one word backward from `cursor`.
pub fn find_word_backward(text: &str, cursor: usize, options: Option<&WordNavigationOptions<'_>>) -> usize {
    let cursor = clamp_boundary(text, cursor);
    if cursor == 0 {
        return 0;
    }

    let text_before_cursor = &text[..cursor];
    let is_atomic = options.and_then(|options| options.is_atomic_segment);
    let mut segments = segment_text(text_before_cursor, options);
    let mut new_cursor = cursor;

    while let Some(last) = segments.last() {
        if is_atomic_segment(is_atomic, &last.segment) || !is_whitespace_char(&last.segment) {
            break;
        }
        new_cursor -= last.segment.len();
        segments.pop();
    }

    let Some(last) = segments.last() else {
        return new_cursor;
    };

    if is_atomic_segment(is_atomic, &last.segment) {
        new_cursor -= last.segment.len();
    } else if last.is_word_like {
        new_cursor -= word_backward_offset(&last.segment);
    } else {
        while let Some(segment) = segments.last() {
            if is_atomic_segment(is_atomic, &segment.segment)
                || segment.is_word_like
                || is_whitespace_char(&segment.segment)
            {
                break;
            }
            new_cursor -= segment.segment.len();
            segments.pop();
        }
    }

    new_cursor
}

/// Find the cursor position after moving one word forward from `cursor`.
pub fn find_word_forward(text: &str, cursor: usize, options: Option<&WordNavigationOptions<'_>>) -> usize {
    let cursor = clamp_boundary(text, cursor);
    if cursor >= text.len() {
        return text.len();
    }

    let text_after_cursor = &text[cursor..];
    let is_atomic = options.and_then(|options| options.is_atomic_segment);
    let segments = segment_text(text_after_cursor, options);
    let mut iterator = segments.iter();
    let mut new_cursor = cursor;

    let mut next = iterator.next();
    while let Some(segment) = next {
        if is_atomic_segment(is_atomic, &segment.segment) || !is_whitespace_char(&segment.segment) {
            break;
        }
        new_cursor += segment.segment.len();
        next = iterator.next();
    }

    let Some(segment) = next else {
        return new_cursor;
    };

    if is_atomic_segment(is_atomic, &segment.segment) {
        new_cursor += segment.segment.len();
    } else if segment.is_word_like {
        new_cursor += word_forward_offset(&segment.segment);
    } else {
        let mut run = Some(segment);
        while let Some(segment) = run {
            if is_atomic_segment(is_atomic, &segment.segment)
                || segment.is_word_like
                || is_whitespace_char(&segment.segment)
            {
                break;
            }
            new_cursor += segment.segment.len();
            run = iterator.next();
        }
    }

    new_cursor
}

fn segment_text(text: &str, options: Option<&WordNavigationOptions<'_>>) -> Vec<WordSegment> {
    match options.and_then(|options| options.segment) {
        Some(segment) => segment(text),
        None => icu_segments(text),
    }
}

fn icu_segments(text: &str) -> Vec<WordSegment> {
    let segmenter = WordSegmenter::new_auto(WordBreakInvariantOptions::default());
    let mut iterator = segmenter.segment_str(text);
    let mut start = 0;
    let mut segments: Vec<WordSegment> = Vec::new();

    while let Some(end) = iterator.next() {
        if end > start {
            let segment = &text[start..end];
            let word_type = iterator.word_type();
            // ICU4X tags ExtendNumLet/Extend runs (for example a lone
            // underscore) as Letter; ICU4C reports WORD_NONE unless the
            // segment contains an actual letter or a number.
            let is_word_like = word_type.is_word_like()
                && (matches!(word_type, WordType::Number) || segment.chars().any(char::is_alphabetic));
            // ICU4X splits combining marks after Han where ICU4C (Node) keeps
            // them attached (UAX #29 WB4). Merge those mark runs back, except
            // after a line break where WB3 keeps the break.
            if segment.chars().next().is_some_and(is_mark)
                && let Some(previous) = segments.last_mut()
                && !previous.segment.ends_with(['\r', '\n', '\u{2028}', '\u{2029}'])
            {
                previous.segment.push_str(segment);
            } else {
                segments.push(WordSegment {
                    segment: segment.to_string(),
                    is_word_like,
                });
            }
        }
        start = end;
    }

    segments
}

fn is_atomic_segment(is_atomic: Option<&dyn Fn(&str) -> bool>, segment: &str) -> bool {
    is_atomic.is_some_and(|is_atomic| is_atomic(segment))
}

/// Byte distance from the end of a word-like segment to its last ASCII
/// punctuation character, matching `PUNCTUATION_REGEX` in the source.
fn word_backward_offset(segment: &str) -> usize {
    match segment
        .char_indices()
        .rfind(|(_, character)| is_punctuation_char_value(*character))
    {
        Some((index, character)) => segment.len() - (index + character.len_utf8()),
        None => segment.len(),
    }
}

/// Byte distance to the first ASCII punctuation character in a word-like
/// segment, or the whole segment length when none exists.
fn word_forward_offset(segment: &str) -> usize {
    segment
        .char_indices()
        .find(|(_, character)| is_punctuation_char_value(*character))
        .map_or(segment.len(), |(index, _)| index)
}

fn clamp_boundary(text: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(text.len());
    while cursor > 0 && !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}
